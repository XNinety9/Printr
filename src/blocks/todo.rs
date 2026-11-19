//! À faire : une liste à cocher au stylo, écrite dans le bloc ou tirée d'une liste enregistrée de
//! l'appli (« À faire ce week-end »), que la famille complète au fil des jours.

use anyhow::Result;

use crate::doc::{Doc, Style};
use crate::store::{self, TodoList};

fn render(title: Option<&str>, items: &[String]) -> Doc {
    let mut doc = Doc::new();
    doc.header(title.unwrap_or(crate::i18n::t("À faire", "To do")));
    for item in items {
        doc.hanging("[ ] ", item, Style::default());
    }
    doc
}

/// Une liste enregistrée ; `None` si elle a été supprimée depuis : le bloc le dit au lieu de faire
/// échouer tout le ticket.
fn render_list(title: Option<&str>, list: Option<&TodoList>) -> Doc {
    let Some(list) = list else {
        let mut doc = render(title, &[]);
        doc.text(
            crate::i18n::t("Liste introuvable : a-t-elle été supprimée ?", "List not found: was it deleted?"),
            Style::default().small().center(),
        );
        return doc;
    };
    let items: Vec<String> = list.items.iter().map(|i| i.text.clone()).collect();
    let mut doc = render(Some(title.unwrap_or(&list.title)), &items);
    if items.is_empty() {
        doc.text(
            crate::i18n::t("Rien à faire : profitez-en !", "Nothing to do: enjoy!"),
            Style::default().small().center(),
        );
    }
    doc
}

pub fn build(title: Option<&str>, items: &[String], list: Option<&str>) -> Result<Doc> {
    let title = title.filter(|t| !t.trim().is_empty());
    match list.filter(|id| !id.is_empty()) {
        None => Ok(render(title, items)),
        Some(id) => {
            let lists = store::read_data()?.todo_lists;
            Ok(render_list(title, lists.iter().find(|l| l.id == id)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::Op;
    use crate::store::ShoppingItem;

    fn lines(doc: &Doc) -> Vec<&str> {
        doc.ops
            .iter()
            .filter_map(|op| match op {
                Op::Line { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    fn list() -> TodoList {
        let item = |text: &str| ShoppingItem {
            id: text.into(),
            text: text.into(),
            by: "Léo".into(),
            added: chrono::Utc::now(),
        };
        TodoList { id: "abc".into(), title: "Ce week-end".into(), items: vec![item("Tondre"), item("Vélo")] }
    }

    #[test]
    fn items_in_the_block() {
        let doc = render(None, &["Pain".into(), "Lait".into()]);
        assert!(lines(&doc).contains(&"[ ] Pain"));
        assert!(lines(&doc).contains(&"[ ] Lait"));
    }

    #[test]
    fn saved_list_and_its_title() {
        let list = list();
        let doc = render_list(None, Some(&list));
        let lines = lines(&doc);
        assert!(lines.contains(&"[ ] Tondre"));
        assert!(lines.iter().any(|l| l.contains("CE WEEK-END")));
        let doc = render_list(Some("Samedi"), Some(&list));
        assert!(self::lines(&doc).iter().any(|l| l.contains("SAMEDI")));
    }

    #[test]
    fn empty_list_id_means_the_block_lines() {
        let doc = build(None, &["Pain".into()], Some("")).unwrap();
        assert!(lines(&doc).contains(&"[ ] Pain"));
    }

    #[test]
    fn missing_list_still_prints() {
        let doc = render_list(None, None);
        assert!(lines(&doc).iter().any(|l| l.contains("introuvable") || l.contains("not found")));
    }
}
