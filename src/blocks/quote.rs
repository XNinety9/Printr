//! Citation du jour, tirée d'une liste locale de citations françaises connues.

use chrono::{Datelike, NaiveDate};

use crate::doc::{Align, Doc, Style};

const QUOTES: &[(&str, &str)] = &[
    ("Le cœur a ses raisons que la raison ne connaît point.", "Blaise Pascal"),
    ("L'homme n'est qu'un roseau, le plus faible de la nature ; mais c'est un roseau pensant.", "Blaise Pascal"),
    ("Le silence éternel de ces espaces infinis m'effraie.", "Blaise Pascal"),
    ("Je n'ai fait celle-ci plus longue que parce que je n'ai pas eu le loisir de la faire plus courte.", "Blaise Pascal"),
    ("Je pense, donc je suis.", "René Descartes"),
    ("Il faut cultiver notre jardin.", "Voltaire"),
    ("Le doute n'est pas un état bien agréable, mais l'assurance est un état ridicule.", "Voltaire"),
    ("On ne voit bien qu'avec le cœur. L'essentiel est invisible pour les yeux.", "Antoine de Saint-Exupéry"),
    ("Les grandes personnes ne comprennent jamais rien toutes seules.", "Antoine de Saint-Exupéry"),
    ("C'est le temps que tu as perdu pour ta rose qui fait ta rose si importante.", "Antoine de Saint-Exupéry"),
    ("Rien ne sert de courir ; il faut partir à point.", "Jean de La Fontaine"),
    ("On a souvent besoin d'un plus petit que soi.", "Jean de La Fontaine"),
    ("Ce que l'on conçoit bien s'énonce clairement, et les mots pour le dire arrivent aisément.", "Nicolas Boileau"),
    ("Il faut imaginer Sisyphe heureux.", "Albert Camus"),
    ("Au milieu de l'hiver, j'apprenais enfin qu'il y avait en moi un été invincible.", "Albert Camus"),
    ("Mal nommer les choses, c'est ajouter au malheur du monde.", "Albert Camus"),
    ("On ne naît pas femme : on le devient.", "Simone de Beauvoir"),
    ("Un seul être vous manque, et tout est dépeuplé.", "Alphonse de Lamartine"),
    ("Ô temps ! suspends ton vol.", "Alphonse de Lamartine"),
    ("Mon verre n'est pas grand, mais je bois dans mon verre.", "Alfred de Musset"),
    ("Je est un autre.", "Arthur Rimbaud"),
    ("On n'est pas sérieux, quand on a dix-sept ans.", "Arthur Rimbaud"),
    ("Le génie n'est que l'enfance retrouvée à volonté.", "Charles Baudelaire"),
    ("Il faut être toujours ivre. Tout est là.", "Charles Baudelaire"),
    ("La beauté sera convulsive ou ne sera pas.", "André Breton"),
    ("La plus perdue de toutes les journées est celle où l'on n'a pas ri.", "Chamfort"),
    ("Dans les champs de l'observation, le hasard ne favorise que les esprits préparés.", "Louis Pasteur"),
    ("Que sais-je ?", "Michel de Montaigne"),
    ("Parce que c'était lui, parce que c'était moi.", "Michel de Montaigne"),
    ("Chaque homme porte la forme entière de l'humaine condition.", "Michel de Montaigne"),
    ("Tout le monde se plaint de sa mémoire, et personne ne se plaint de son jugement.", "La Rochefoucauld"),
    ("Nous avons tous assez de force pour supporter les maux d'autrui.", "La Rochefoucauld"),
    ("On ne donne rien si libéralement que ses conseils.", "La Rochefoucauld"),
    ("Le pessimisme est d'humeur ; l'optimisme est de volonté.", "Alain"),
    ("Rien n'est plus dangereux qu'une idée, quand on n'a qu'une idée.", "Alain"),
    ("Dis-moi ce que tu manges, je te dirai ce que tu es.", "Jean Anthelme Brillat-Savarin"),
    ("Le style est l'homme même.", "Buffon"),
    ("Vivre sans philosopher, c'est proprement avoir les yeux fermés, sans tâcher jamais de les ouvrir.", "René Descartes"),
];

pub fn build(today: NaiveDate) -> Doc {
    // Une citation différente chaque jour, la même toute la journée.
    let index = (today.num_days_from_ce() as usize * 7) % QUOTES.len();
    let (quote, author) = QUOTES[index];
    let mut doc = Doc::new();
    doc.text(&format!("« {quote} »"), Style::default().center());
    doc.text(&format!("- {author}"), Style::default().small().align(Align::Right));
    doc
}
