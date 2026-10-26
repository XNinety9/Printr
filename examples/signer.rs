//! Signature des releases de printr, au format minisign, sans avoir à installer minisign.
//!
//!   cargo run -q --example signer -- keygen DOSSIER          crée printr.pub et printr.key
//!   cargo run -q --example signer -- sign printr.key FICHIER écrit FICHIER.minisig
//!   cargo run -q --example signer -- verify CLÉ FICHIER      vérifie FICHIER.minisig
//!     (CLÉ : printr.pub, ou la ligne de la clé publique telle que compilée dans printr)
//!
//! Le mot de passe de la clé privée est demandé (ou lu dans `$SIGNER_PASSWORD`). Un mot de passe
//! vide donne une clé non chiffrée : à réserver aux clés jetables du laboratoire.

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use minisign::{KeyPair, PublicKey, PublicKeyBox, SecretKeyBox, SignatureBox};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn password(prompt: &str) -> Result<String> {
    match std::env::var("SIGNER_PASSWORD") {
        Ok(p) => Ok(p),
        Err(_) => Ok(rpassword::prompt_password(prompt)?),
    }
}

fn keygen(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    let (public, secret) = (dir.join("printr.pub"), dir.join("printr.key"));
    if secret.exists() {
        return Err(format!("{} existe déjà : je ne l'écrase pas", secret.display()).into());
    }
    let pw = password("Mot de passe de la clé privée (vide : clé de test non chiffrée) : ")?;
    let KeyPair { pk, sk } = if pw.is_empty() {
        KeyPair::generate_unencrypted_keypair()?
    } else {
        KeyPair::generate_encrypted_keypair(Some(pw))?
    };
    fs::write(&public, pk.to_box()?.into_string())?;
    let mut file = File::create(&secret)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(sk.to_box(None)?.into_string().as_bytes())?;
    let key_line = pk.to_box()?.into_string().lines().nth(1).unwrap_or_default().to_owned();
    println!("Clé publique (à mettre dans printr) : {key_line}");
    println!("Clé privée : {} (à garder secrète, et à sauvegarder hors de cette machine)", secret.display());
    Ok(())
}

fn sign(key: &Path, file: &Path) -> Result<()> {
    let text = fs::read_to_string(key)?;
    // Une clé chiffrée demande son mot de passe ; une clé de test non chiffrée s'ouvre directement.
    let sk = match SecretKeyBox::from_string(&text)?.into_unencrypted_secret_key() {
        Ok(sk) => sk,
        Err(_) => {
            SecretKeyBox::from_string(&text)?.into_secret_key(Some(password("Mot de passe de la clé privée : ")?))?
        }
    };
    let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("fichier");
    let signature = minisign::sign(None, &sk, File::open(file)?, Some(&format!("file:{name}")), None)?;
    let out = format!("{}.minisig", file.display());
    fs::write(&out, signature.to_string())?;
    println!("Signature : {out}");
    Ok(())
}

fn verify(key: &str, file: &Path) -> Result<()> {
    let pk = match fs::read_to_string(key) {
        Ok(text) => PublicKeyBox::from_string(&text)?.into_public_key()?,
        Err(_) => PublicKey::from_base64(key.trim())?,
    };
    let signature = SignatureBox::from_file(format!("{}.minisig", file.display()))?;
    minisign::verify(&pk, &signature, File::open(file)?, true, false, false)?;
    println!("Signature valide : {}", file.display());
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["keygen", dir] => keygen(Path::new(dir)),
        ["sign", key, file] => sign(Path::new(key), Path::new(file)),
        ["verify", key, file] => verify(key, Path::new(file)),
        _ => Err("usage : signer keygen DOSSIER | signer sign CLÉ FICHIER | signer verify CLÉ FICHIER".into()),
    }
}
