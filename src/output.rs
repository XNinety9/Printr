//! Destinations d'impression : périphérique USB, TCP (imprimante réseau, émulateur) ou fichier.

use std::cell::RefCell;
use std::fs::OpenOptions;
use std::io::Write;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

use anyhow::{Context, Result};
use escpos::driver::Driver;
use escpos::printer::Printer;
use escpos::printer_options::PrinterOptions;
use escpos::utils::{PageCode, Protocol};

use crate::doc::Doc;
use crate::i18n::t;

pub enum Target {
    Device(PathBuf),
    Tcp(String),
    Dump(PathBuf),
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Target::Device(path) => write!(f, "{}", path.display()),
            Target::Tcp(addr) => write!(f, "tcp://{addr}"),
            Target::Dump(path) => write!(f, "{} ({})", path.display(), t("fichier", "file")),
        }
    }
}

impl Target {
    /// Ouvre la destination, envoie le document puis la referme.
    pub fn send(&self, doc: &Doc, cut: bool) -> Result<()> {
        self.write(&render(doc, cut)?)
    }

    /// Envoie des octets ESC/POS déjà prêts (ticket mis en file d'attente, par exemple).
    pub fn write(&self, bytes: &[u8]) -> Result<()> {
        match self {
            Target::Tcp(addr) => {
                let mut stream = connect(addr, Duration::from_secs(5))?;
                stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
                stream
                    .write_all(bytes)
                    .with_context(|| tr!("envoi vers {addr} interrompu", "sending to {addr} interrupted"))?;
                stream.flush()?;
                Ok(())
            }
            Target::Dump(path) => std::fs::write(path, bytes)
                .with_context(|| tr!("impossible de créer {}", "cannot create {}", path.display())),
            Target::Device(path) => write_device(path, bytes),
        }
    }

    /// L'état de l'imprimante. USB : le périphérique existe (udev le crée quand l'imprimante est
    /// branchée et allumée), et le pilote lit l'état du port (papier, erreur) par une requête de
    /// contrôle, sans rien envoyer à l'imprimante : rien ne peut rester en attente et bloquer la
    /// liaison, contrairement aux requêtes ESC/POS (`DLE EOT`), auxquelles elle ne répond plus
    /// capot ouvert. Réseau : elle accepte une connexion ; fichier : toujours prête.
    /// `None` : inconnu pour l'instant (périphérique occupé par un envoi en cours).
    pub fn status(&self) -> Option<Status> {
        match self {
            Target::Device(path) => device_status(path),
            Target::Tcp(addr) => {
                Some(if connect(addr, Duration::from_millis(1500)).is_ok() { Status::Ready } else { Status::Missing })
            }
            Target::Dump(_) => Some(Status::Ready),
        }
    }
}

/// Ce que l'imprimante peut faire en ce moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Ready,
    /// En erreur sans manquer de papier : en pratique, le capot ouvert (TM-T88V).
    CoverOpen,
    PaperOut,
    /// Éteinte, débranchée ou injoignable.
    Missing,
}

impl Status {
    /// Le code publié dans l'API et sur MQTT.
    pub fn code(self) -> &'static str {
        match self {
            Status::Ready => "ready",
            Status::CoverOpen => "cover_open",
            Status::PaperOut => "paper_out",
            Status::Missing => "missing",
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Status::Ready => t("imprimante prête", "printer ready"),
            Status::CoverOpen => t("capot ouvert", "cover open"),
            Status::PaperOut => t("plus de papier", "out of paper"),
            Status::Missing => t("imprimante absente", "printer missing"),
        }
    }
}

/// État du port d'une imprimante USB (ioctl `LPGETSTATUS` du pilote usblp), lu sur la TM-T88V :
/// 0x18 prête, 0x10 capot ouvert (erreur), 0x30 plus de papier (et erreur).
#[cfg(target_os = "linux")]
fn device_status(path: &Path) -> Option<Status> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    const LPGETSTATUS: libc::c_ulong = 0x060b;
    const PAPER_OUT: libc::c_int = 0x20;
    const NO_ERROR: libc::c_int = 0x08;
    let file = match OpenOptions::new().write(true).custom_flags(libc::O_NONBLOCK).open(path) {
        Ok(file) => file,
        // usblp n'accepte qu'une ouverture à la fois : un envoi est en cours.
        Err(e) if e.raw_os_error() == Some(libc::EBUSY) => return None,
        // Là mais pas ouvrable (droits) : l'envoi le dira.
        Err(_) => return Some(if path.exists() { Status::Ready } else { Status::Missing }),
    };
    let mut port: libc::c_int = 0;
    // SAFETY: LPGETSTATUS écrit un int à l'adresse donnée, valide le temps de l'appel.
    if unsafe { libc::ioctl(file.as_raw_fd(), LPGETSTATUS as _, &mut port) } != 0 {
        // Pas une imprimante USB (fichier, tube) : on ne sait rien de plus.
        return Some(Status::Ready);
    }
    Some(if port & PAPER_OUT != 0 {
        Status::PaperOut
    } else if port & NO_ERROR == 0 {
        Status::CoverOpen
    } else {
        Status::Ready
    })
}

#[cfg(not(target_os = "linux"))]
fn device_status(path: &Path) -> Option<Status> {
    Some(if path.exists() { Status::Ready } else { Status::Missing })
}

/// Au-delà, une écriture est considérée comme bloquée (imprimante allumée mais en défaut).
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);

/// Fin d'un envoi : le fil d'écriture y dépose son résultat.
pub type Done = Receiver<std::io::Result<()>>;

/// L'imprimante a accepté la connexion mais ne prend plus les données (capot ouvert, plus de
/// papier) : l'envoi reste en suspens et aboutira quand elle sera prête. Le ticket ne doit donc pas
/// être renvoyé : il sortirait deux fois. `take` donne de quoi savoir quand il est enfin sorti.
#[derive(Debug)]
pub struct Stalled(Mutex<Option<Done>>);

impl Stalled {
    pub fn message() -> &'static str {
        t(
            "l'imprimante ne prend plus rien (capot, papier ?) : le ticket sortira quand elle sera prête",
            "the printer is not taking anything (cover, paper?): the ticket will come out when it is ready",
        )
    }

    /// La fin de l'envoi, une seule fois.
    pub fn take(&self) -> Option<Done> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).take()
    }
}

impl std::fmt::Display for Stalled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(Self::message())
    }
}

impl std::error::Error for Stalled {}

fn connect(addr: &str, timeout: Duration) -> Result<TcpStream> {
    let target = addr
        .to_socket_addrs()
        .with_context(|| {
            tr!("adresse invalide : {addr} (hôte:port attendu)", "invalid address: {addr} (host:port expected)")
        })?
        .next()
        .with_context(|| tr!("adresse introuvable : {addr}", "address not found: {addr}"))?;
    TcpStream::connect_timeout(&target, timeout)
        .with_context(|| tr!("impossible de se connecter à {addr}", "cannot connect to {addr}"))
}

/// Écrit sur le périphérique USB dans un fil à part : si l'imprimante bloque l'écriture (plus de
/// papier, capot ouvert), on abandonne au bout de `WRITE_TIMEOUT` au lieu de rester coincé.
fn write_device(path: &Path, bytes: &[u8]) -> Result<()> {
    write_device_within(path, bytes, WRITE_TIMEOUT)
}

fn write_device_within(path: &Path, bytes: &[u8], timeout: Duration) -> Result<()> {
    let mut file = OpenOptions::new().write(true).open(path).with_context(|| {
        tr!(
            "impossible d'ouvrir {} (imprimante branchée ? droits udev ?)",
            "cannot open {} (printer plugged in? udev permissions?)",
            path.display()
        )
    })?;
    let bytes = bytes.to_vec();
    let (done, wait) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = done.send(file.write_all(&bytes).and_then(|()| file.flush()));
    });
    match wait.recv_timeout(timeout) {
        Ok(result) => result.with_context(|| tr!("écriture sur {} impossible", "cannot write to {}", path.display())),
        Err(RecvTimeoutError::Timeout) => Err(Stalled(Mutex::new(Some(wait))).into()),
        Err(RecvTimeoutError::Disconnected) => {
            anyhow::bail!("{}", tr!("écriture sur {} interrompue", "writing to {} interrupted", path.display()))
        }
    }
}

/// Octets ESC/POS d'un ticket, fabriqués en mémoire.
pub fn render(doc: &Doc, cut: bool) -> Result<Vec<u8>> {
    let buffer = Rc::new(RefCell::new(Vec::new()));
    let driver = MemoryDriver(Rc::clone(&buffer));
    // PC858 = Europe de l'Ouest avec le symbole € ; le texte est encodé par `cp858`.
    let options = PrinterOptions::new(Some(PageCode::PC858), None, 42);
    let mut printer = Printer::new(driver, Protocol::default(), Some(options));
    printer.init()?.page_code(PageCode::PC858)?;
    doc.render(&mut printer)?;
    // Un ticket qui finit par un bloc coupe est déjà coupé.
    if cut && !doc.ends_with_cut() {
        printer.feeds(3)?.print_cut()?;
    } else {
        printer.print()?;
    }
    drop(printer);
    Ok(Rc::try_unwrap(buffer).map(RefCell::into_inner).unwrap_or_default())
}

/// Pilote escpos qui garde les octets en mémoire au lieu de les envoyer.
struct MemoryDriver(Rc<RefCell<Vec<u8>>>);

impl Driver for MemoryDriver {
    fn name(&self) -> String {
        "memory".to_owned()
    }

    fn write(&self, data: &[u8]) -> escpos::errors::Result<()> {
        self.0.borrow_mut().extend_from_slice(data);
        Ok(())
    }

    fn read(&self, _buf: &mut [u8]) -> escpos::errors::Result<usize> {
        Ok(0)
    }

    fn flush(&self) -> escpos::errors::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::Style;

    #[test]
    fn rendered_bytes_start_with_init_and_end_with_cut() {
        let mut doc = Doc::new();
        doc.text("Bonjour", Style::default());
        let bytes = render(&doc, true).unwrap();
        assert!(bytes.starts_with(&[0x1b, 0x40]), "ESC @ en tête");
        assert!(bytes.windows(2).any(|w| w == [0x1d, 0x56]), "coupe (GS V)");
        let path = std::env::temp_dir().join(format!("printr-test-{}.bin", std::process::id()));
        Target::Dump(path.clone()).write(&bytes).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let _ = std::fs::remove_file(path);
    }

    /// Un texte qui contient une commande ESC/POS (ici, ouvrir le tiroir-caisse) l'imprime en clair au
    /// mieux, sans jamais l'envoyer à l'imprimante.
    #[test]
    fn commands_hidden_in_text_are_not_sent() {
        let mut doc = Doc::new();
        doc.text("avant\x1bp\x00\x19\x19après", Style::default());
        doc.line("\x1d\x56\x00coupe", Style::default());
        let bytes = render(&doc, false).unwrap();
        assert!(!bytes.windows(2).any(|w| w == [0x1b, b'p']), "ESC p transmis");
        assert!(!bytes.windows(3).any(|w| w == [0x1d, 0x56, 0x00]), "GS V transmis");
    }

    #[test]
    fn cut_op_cuts_mid_ticket_without_doubling_the_final_cut() {
        let cuts = |doc: &Doc, cut| render(doc, cut).unwrap().windows(2).filter(|w| *w == [0x1d, 0x56]).count();
        let mut doc = Doc::new();
        doc.text("Un", Style::default()).cut().text("Deux", Style::default());
        assert_eq!(cuts(&doc, true), 2);
        assert_eq!(cuts(&doc, false), 1);
        doc.cut();
        assert_eq!(cuts(&doc, true), 2, "la coupe finale est déjà là");
    }

    /// Capot ouvert simulé par un tube nommé qu'on ne lit pas : l'envoi passe en suspens, puis
    /// aboutit, en entier et une seule fois, quand on se remet à lire.
    #[cfg(unix)]
    #[test]
    fn stalled_write_finishes_when_the_printer_takes_data_again() {
        use std::io::Read;
        let fifo = std::env::temp_dir().join(format!("printr-test-{}.fifo", std::process::id()));
        let _ = std::fs::remove_file(&fifo);
        assert!(std::process::Command::new("mkfifo").arg(&fifo).status().unwrap().success());
        let (resume, resumed) = std::sync::mpsc::channel::<()>();
        let reader = {
            let fifo = fifo.clone();
            std::thread::spawn(move || {
                let mut file = std::fs::File::open(fifo).unwrap();
                resumed.recv().unwrap();
                let mut all = Vec::new();
                file.read_to_end(&mut all).unwrap();
                all.len()
            })
        };
        // Bien plus que la mémoire tampon d'un tube (64 Kio) : l'écriture bloque.
        let bytes = vec![b'x'; 512 * 1024];
        let err = write_device_within(&fifo, &bytes, Duration::from_millis(300)).unwrap_err();
        let done = err.downcast_ref::<Stalled>().expect("en suspens").take().expect("fin de l'envoi");
        resume.send(()).unwrap();
        assert!(done.recv_timeout(Duration::from_secs(10)).unwrap().is_ok());
        assert_eq!(reader.join().unwrap(), bytes.len());
        let _ = std::fs::remove_file(fifo);
    }

    #[test]
    fn missing_device_is_not_available() {
        assert_eq!(Target::Device("/nonexistent/printr-test".into()).status(), Some(Status::Missing));
        assert_eq!(Target::Dump("/tmp/x".into()).status(), Some(Status::Ready));
        // Un fichier ordinaire n'a pas d'état de port : prêt.
        let path = std::env::temp_dir().join(format!("printr-status-{}", std::process::id()));
        std::fs::write(&path, b"").unwrap();
        assert_eq!(Target::Device(path.clone()).status(), Some(Status::Ready));
        let _ = std::fs::remove_file(path);
    }
}
