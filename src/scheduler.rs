//! Planificateur : imprime les presets à l'heure prévue.
//!
//! Une échéance manquée de plus de `GRACE` (Pi éteint, imprimante débranchée…) n'est pas
//! rattrapée : un horoscope du matin ne doit pas sortir l'après-midi.

use std::sync::Arc;
use std::time::Duration as StdDuration;

use chrono::{Duration, Local, Utc};

use crate::blocks::Ticket;
use crate::server::App;

const GRACE: Duration = Duration::minutes(15);
const TICK: StdDuration = StdDuration::from_secs(20);

pub fn spawn(app: Arc<App>) {
    std::thread::spawn(move || loop {
        run_due(&app);
        std::thread::sleep(TICK);
    });
}

fn run_due(app: &App) {
    let now = Local::now();
    // Repère les échéances et les marque comme faites avant d'imprimer : un ticket lent
    // à construire ne doit jamais partir deux fois.
    let due: Vec<(String, serde_json::Value)> = {
        let mut store = app.store.lock().unwrap_or_else(|e| e.into_inner());
        let mut due = Vec::new();
        for preset in &mut store.data.presets {
            for schedule in &mut preset.schedules {
                if schedule.due(now, GRACE).is_some() {
                    schedule.last_run = Some(Utc::now());
                    due.push((preset.name.clone(), preset.ticket.clone()));
                }
            }
        }
        if !due.is_empty() {
            if let Err(e) = store.save() {
                eprintln!("planification non enregistrée : {e:#}");
            }
        }
        due
    };

    for (name, ticket) in due {
        match serde_json::from_value::<Ticket>(ticket) {
            Ok(ticket) => {
                let printed = app.print(&ticket, "planification", &name);
                match printed.failure {
                    None => eprintln!("planification « {name} » imprimée"),
                    Some(e) => eprintln!("planification « {name} » en échec : {e}"),
                }
            }
            Err(e) => eprintln!("planification « {name} » : ticket invalide ({e})"),
        }
    }
}
