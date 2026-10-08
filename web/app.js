// Printr — interface web : composer, imprimer et planifier des tickets.
// Preact + htm, sans étape de compilation.

import { html, render, useState, useEffect, useRef, useMemo, useCallback } from '/vendor/preact-htm.mjs';

// ============================================================================
// Outils
// ============================================================================

const listeners = {};
const bus = {
  on(event, fn) { (listeners[event] ||= new Set()).add(fn); return () => listeners[event].delete(fn); },
  emit(event, data) { listeners[event]?.forEach((fn) => fn(data)); },
};

async function api(method, path, body) {
  const opts = { method, headers: {}, credentials: 'same-origin' };
  if (body instanceof Blob) {
    opts.body = body;
    opts.headers['Content-Type'] = body.type || 'application/octet-stream';
  } else if (body !== undefined) {
    opts.body = JSON.stringify(body);
    opts.headers['Content-Type'] = 'application/json';
  }
  let res;
  try {
    res = await fetch(path, opts);
  } catch {
    throw new Error('Impossible de joindre Printr. Vérifie la connexion.');
  }
  let data = null;
  try { data = await res.json(); } catch { /* corps vide */ }
  if (res.status === 401 && path !== '/api/login') bus.emit('unauthorized');
  if (!res.ok) throw new Error(data?.error ? capitalize(data.error) : `Erreur ${res.status}`);
  return data;
}

const toast = (message, kind = 'ok', emoji) => bus.emit('toast', { message, kind, emoji });
const capitalize = (s) => (s ? s[0].toUpperCase() + s.slice(1) : s);
const uid = () => Math.random().toString(36).slice(2, 10);
const clone = (v) => JSON.parse(JSON.stringify(v));

function useHash() {
  const [hash, setHash] = useState(location.hash || '#/');
  useEffect(() => {
    const onChange = () => { setHash(location.hash || '#/'); window.scrollTo({ top: 0 }); };
    addEventListener('hashchange', onChange);
    return () => removeEventListener('hashchange', onChange);
  }, []);
  return hash;
}

function useDebounced(value, delay) {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setDebounced(value), delay);
    return () => clearTimeout(t);
  }, [value, delay]);
  return debounced;
}

const store = {
  get(key, fallback) { try { return JSON.parse(localStorage.getItem(key)) ?? fallback; } catch { return fallback; } },
  set(key, value) { try { localStorage.setItem(key, JSON.stringify(value)); } catch { /* navigation privée */ } },
  del(key) { try { localStorage.removeItem(key); } catch { /* idem */ } },
};

// Réduit une photo dans le navigateur avant l'envoi : rapide même en 4G.
async function resizeImage(file, max = 1024) {
  let source;
  try {
    source = await createImageBitmap(file, { imageOrientation: 'from-image' });
  } catch {
    source = await new Promise((resolve, reject) => {
      const img = new Image();
      img.onload = () => resolve(img);
      img.onerror = () => reject(new Error('Image illisible'));
      img.src = URL.createObjectURL(file);
    });
  }
  const scale = Math.min(1, max / source.width);
  const canvas = document.createElement('canvas');
  canvas.width = Math.round(source.width * scale);
  canvas.height = Math.round(source.height * scale);
  const ctx = canvas.getContext('2d');
  ctx.fillStyle = '#fff';
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.drawImage(source, 0, 0, canvas.width, canvas.height);
  return new Promise((resolve) => canvas.toBlob(resolve, 'image/jpeg', 0.9));
}

async function uploadPhoto(file) {
  const blob = await resizeImage(file);
  const { id } = await api('POST', '/api/images', blob);
  return id;
}

// ============================================================================
// Icônes
// ============================================================================

const ICONS = {
  home: '<path d="M3 10.5 12 3l9 7.5V20a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1z"/>',
  ticket: '<path d="M6 3h12v18l-2-1.5-2 1.5-2-1.5-2 1.5-2-1.5L6 21z"/><path d="M9 8h6M9 12h6M9 16h3"/>',
  heart: '<path d="M12 20s-7-4.4-9-9a5 5 0 0 1 9-3 5 5 0 0 1 9 3c-2 4.6-9 9-9 9z"/>',
  history: '<path d="M3 12a9 9 0 1 0 3-6.7L3 8"/><path d="M3 3v5h5"/><path d="M12 7v5l3 2"/>',
  clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  printer: '<path d="M6 9V3h12v6"/><rect x="3" y="9" width="18" height="9" rx="2"/><path d="M6 14h12v7H6z"/>',
  trash: '<path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3"/>',
  up: '<path d="m6 15 6-6 6 6"/>',
  down: '<path d="m6 9 6 6 6-6"/>',
  copy: '<rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15V5a2 2 0 0 1 2-2h10"/>',
  edit: '<path d="M4 20h4L19 9l-4-4L4 16z"/><path d="m13.5 6.5 4 4"/>',
  x: '<path d="M18 6 6 18M6 6l12 12"/>',
  check: '<path d="m5 12 5 5L20 7"/>',
  camera: '<path d="M4 8h3l2-3h6l2 3h3a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V9a1 1 0 0 1 1-1z"/><circle cx="12" cy="13" r="4"/>',
  eye: '<path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/>',
  logout: '<path d="M15 4h4a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1h-4M10 17l-5-5 5-5M5 12h11"/>',
  users: '<circle cx="9" cy="8" r="4"/><path d="M2 21a7 7 0 0 1 14 0"/><path d="M16 4a4 4 0 0 1 0 8M22 21a7 7 0 0 0-5-6.7"/>',
  save: '<path d="M6 3h12v18l-6-4-6 4z"/>',
  sliders: '<path d="M4 6h9M17 6h3M4 12h3M11 12h9M4 18h11M19 18h1"/><circle cx="15" cy="6" r="2"/><circle cx="9" cy="12" r="2"/><circle cx="17" cy="18" r="2"/>',
  send: '<path d="M22 2 11 13M22 2l-7 20-4-9-9-4z"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>',
  spark: '<path d="M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8z"/>',
};

const Icon = ({ name, size = 20 }) => html`<svg class="icon" width=${size} height=${size} viewBox="0 0 24 24" fill="none"
  stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"
  dangerouslySetInnerHTML=${{ __html: ICONS[name] || '' }} />`;

const Spinner = () => html`<span class="spinner" role="status" aria-label="Chargement"></span>`;

const Avatar = ({ user, size = '' }) => html`<span class="avatar ${size}" style=${{ background: user?.color || '#999' }}
  title=${user?.name}>${(user?.name || '?')[0].toUpperCase()}</span>`;

// ============================================================================
// Catalogue des blocs
// ============================================================================

const SIGNS = [
  ['belier', 'Bélier ♈'], ['taureau', 'Taureau ♉'], ['gemeaux', 'Gémeaux ♊'], ['cancer', 'Cancer ♋'],
  ['lion', 'Lion ♌'], ['vierge', 'Vierge ♍'], ['balance', 'Balance ♎'], ['scorpion', 'Scorpion ♏'],
  ['sagittaire', 'Sagittaire ♐'], ['capricorne', 'Capricorne ♑'], ['verseau', 'Verseau ♒'], ['poissons', 'Poissons ♓'],
];
const TONES = [['serieux', 'Sérieux'], ['farfelu', 'Farfelu'], ['vachard', 'Vachard'], ['insultant', 'Insultant']];
const LEVELS = [['facile', 'Facile'], ['moyen', 'Moyen'], ['difficile', 'Difficile']];
const WORD_THEMES = [['', 'Au hasard'], ['animaux', 'Animaux'], ['fruits_legumes', 'Fruits et légumes'], ['cuisine', 'Cuisine'], ['nature', 'Nature'], ['sport', 'Sport'], ['metiers', 'Métiers'], ['maison', 'Maison'], ['voyage', 'Voyage'], ['musique', 'Musique'], ['ecole', 'École'], ['developpement', 'Développement'], ['devops', 'DevOps'], ['reseaux', 'Réseaux'], ['ia', 'Intelligence artificielle']];
const PICTOS = [['coeur', '❤️'], ['etoile', '⭐'], ['soleil', '☀️'], ['fleur', '🌸'], ['sourire', '😊']];
const label = (options, value) => options.find(([v]) => v === value)?.[1] ?? value;
const inDays = (n) => new Date(Date.now() + n * 864e5).toISOString().slice(0, 10);

const G = { layout: 'Mise en page', daily: 'Au quotidien', fun: 'Jeux & défis', info: 'Infos', org: 'Organisation' };

const BLOCKS = [
  {
    type: 'title', label: 'Titre', emoji: '🔠', group: G.layout, desc: 'Un grand titre centré',
    defaults: { text: 'Bonjour !', size: 2 },
    fields: [
      { key: 'text', label: 'Texte', kind: 'text' },
      { key: 'size', label: 'Taille', kind: 'segmented', options: [[1, 'S'], [2, 'M'], [3, 'L'], [4, 'XL']] },
    ],
    summary: (b) => b.text,
  },
  {
    type: 'text', label: 'Texte', emoji: '📝', group: G.layout, desc: 'Un paragraphe libre',
    defaults: { text: '', align: 'left' },
    fields: [
      { key: 'text', label: 'Texte', kind: 'textarea', placeholder: 'Écris ce que tu veux…' },
      { key: 'align', label: 'Alignement', kind: 'segmented', options: [['left', 'Gauche'], ['center', 'Centré'], ['right', 'Droite']] },
      { key: 'size', label: 'Taille', kind: 'segmented', options: [[1, 'Normale'], [2, 'Grande'], [3, 'Énorme']] },
      { key: 'bold', label: 'Gras', kind: 'toggle' },
      { key: 'underline', label: 'Souligné', kind: 'toggle' },
      { key: 'reverse', label: 'Inversé (blanc sur noir)', kind: 'toggle' },
      { key: 'small', label: 'Petite police', kind: 'toggle' },
    ],
    summary: (b) => b.text || 'Vide',
  },
  { type: 'date', label: 'Date', emoji: '📅', group: G.layout, desc: 'La date du jour, en toutes lettres', defaults: {}, fields: [] },
  {
    type: 'separator', label: 'Séparateur', emoji: '➖', group: G.layout, desc: 'Une ligne de séparation',
    defaults: { style: '-' },
    fields: [{ key: 'style', label: 'Style', kind: 'segmented', options: [['-', '—'], ['=', '='], ['═', '═'], ['─', '─'], ['·', '·'], ['*', '*'], ['~', '~']] }],
  },
  {
    type: 'feed', label: 'Espace', emoji: '↕️', group: G.layout, desc: "De l'air entre deux blocs",
    defaults: { lines: 1 },
    fields: [{ key: 'lines', label: 'Lignes vides', kind: 'number', min: 1, max: 10 }],
    summary: (b) => `${b.lines} ligne${b.lines > 1 ? 's' : ''}`,
  },
  {
    type: 'image', label: 'Image', emoji: '🖼️', group: G.layout, desc: 'Une photo, imprimée en noir et blanc',
    defaults: { dither: true },
    fields: [
      { key: 'upload', label: 'Photo', kind: 'image' },
      { key: 'url', label: "… ou adresse web d'une image", kind: 'text', placeholder: 'https://…' },
      { key: 'dither', label: 'Tramage', kind: 'toggle', help: 'Idéal pour les photos. Désactive-le pour un logo ou un dessin.' },
    ],
    normalize: (d) => { if (d.upload) delete d.url; return d; },
    summary: (b) => (b.upload ? 'Photo' : b.url ? 'Image du web' : 'Aucune image'),
  },
  {
    type: 'qr', label: 'QR code', emoji: '🔳', group: G.layout, desc: 'Un lien à scanner',
    defaults: { data: '', size: 6 },
    fields: [
      { key: 'data', label: 'Lien ou texte', kind: 'text', placeholder: 'https://…' },
      { key: 'caption', label: 'Légende', kind: 'text', placeholder: 'Optionnelle' },
      { key: 'size', label: 'Taille', kind: 'number', min: 2, max: 12 },
    ],
    summary: (b) => b.data || 'Vide',
  },
  {
    type: 'picto', label: 'Pictogramme', emoji: '💖', group: G.layout, desc: 'Cœur, étoile, soleil, fleur…',
    defaults: { shape: 'coeur', size: 'moyen', count: 1 },
    fields: [
      { key: 'shape', label: 'Forme', kind: 'icons', options: PICTOS },
      { key: 'size', label: 'Taille', kind: 'segmented', options: [['petit', 'Petit'], ['moyen', 'Moyen'], ['grand', 'Grand']] },
      { key: 'count', label: 'Combien ?', kind: 'number', min: 1, max: 7 },
    ],
    summary: (b) => `${label(PICTOS, b.shape)} × ${b.count}`,
  },
  {
    type: 'weather', label: 'Météo', emoji: '🌤️', group: G.daily, desc: 'Le temps du jour et des suivants',
    defaults: { location: 'Lyon', days: 1 },
    fields: [
      { key: 'location', label: 'Ville', kind: 'text', placeholder: 'Lyon, ou « 45.76,4.83 »' },
      { key: 'days', label: 'Prévisions', kind: 'segmented', options: [[1, "Aujourd'hui"], [3, '3 jours'], [5, '5 jours'], [7, '7 jours']] },
    ],
    summary: (b) => b.location,
  },
  {
    type: 'barnum', label: 'Horoscope', emoji: '🌟', group: G.daily, desc: 'Le vrai ciel du jour, hors ligne et gratuit',
    defaults: { sign: 'lion', sky: false },
    fields: [
      { key: 'sign', label: 'Signe', kind: 'select', options: SIGNS },
      { key: 'birth_date', label: '… ou date de naissance', kind: 'date', help: 'Si elle est indiquée, Barnum en déduit le signe.' },
      { key: 'sky', label: 'Afficher le ciel du jour', kind: 'toggle', help: 'La position des planètes, et celles qui sont rétrogrades.' },
    ],
    summary: (b) => (b.birth_date ? `Né le ${b.birth_date}` : label(SIGNS, b.sign)),
  },
  {
    type: 'horoscope', label: 'Horoscope Claude', emoji: '🔮', group: G.daily, desc: 'Sérieux, farfelu… ou insultant', ai: true,
    defaults: { sign: 'lion', tone: 'serieux' },
    fields: [
      { key: 'sign', label: 'Signe', kind: 'select', options: SIGNS },
      { key: 'tone', label: 'Ton', kind: 'segmented', options: TONES },
    ],
    summary: (b) => `${label(SIGNS, b.sign)} · ${label(TONES, b.tone)}`,
  },
  { type: 'word_of_the_day', label: 'Mot du jour', emoji: '📖', group: G.daily, desc: 'Un mot rare et sa définition', ai: true, defaults: {}, fields: [] },
  { type: 'saint', label: 'Saint du jour', emoji: '😇', group: G.daily, desc: 'La fête du jour', defaults: {}, fields: [] },
  { type: 'quote', label: 'Citation', emoji: '💬', group: G.daily, desc: 'Une citation française célèbre', defaults: {}, fields: [] },
  { type: 'moon', label: 'Lune', emoji: '🌙', group: G.daily, desc: 'Phase et prochaines lunes', defaults: {}, fields: [] },
  {
    type: 'sun', label: 'Soleil', emoji: '🌅', group: G.daily, desc: 'Lever, coucher, durée du jour',
    defaults: { location: 'Lyon' },
    fields: [{ key: 'location', label: 'Ville', kind: 'text' }],
    summary: (b) => b.location,
  },
  {
    type: 'air_quality', label: "Qualité de l'air", emoji: '🍃', group: G.daily, desc: 'Pollution et pollens',
    defaults: { location: 'Lyon' },
    fields: [{ key: 'location', label: 'Ville', kind: 'text' }],
    summary: (b) => b.location,
  },
  {
    type: 'holidays', label: 'Jours fériés', emoji: '🏖️', group: G.daily, desc: 'Les prochains jours fériés',
    defaults: { zone: 'metropole', count: 1 },
    fields: [
      { key: 'zone', label: 'Zone', kind: 'segmented', options: [['metropole', 'Métropole'], ['alsace-moselle', 'Alsace-Moselle']] },
      { key: 'count', label: 'Combien ?', kind: 'number', min: 1, max: 12 },
    ],
  },
  {
    type: 'countdown', label: 'Compte à rebours', emoji: '⏳', group: G.daily, desc: '« J-12 avant les vacances »',
    defaults: { label: 'Vacances', date: inDays(30) },
    fields: [
      { key: 'label', label: 'Événement', kind: 'text' },
      { key: 'date', label: 'Date', kind: 'date' },
    ],
    summary: (b) => `${b.label} · ${b.date}`,
  },
  {
    type: 'riddle', label: 'Énigme', emoji: '🧩', group: G.fun, desc: '125 énigmes, réponse à l’envers',
    defaults: { answer: 'envers' },
    fields: [
      { key: 'kind', label: 'Famille', kind: 'select', options: [['', 'Toutes'], ['devinette', 'Devinettes'], ['charade', 'Charades'], ['logique', 'Logique'], ['calcul', 'Calcul']] },
      { key: 'answer', label: 'Réponse', kind: 'select', options: [['envers', "À l'envers, en bas"], ['lendemain', 'Le lendemain'], ['dessous', 'Juste en dessous'], ['aucune', 'Pas de réponse']] },
    ],
  },
  {
    type: 'workout', label: 'Défi sportif', emoji: '💪', group: G.fun, desc: 'Sans équipement, trois niveaux',
    defaults: { level: 'moyen' },
    fields: [{ key: 'level', label: 'Niveau', kind: 'segmented', options: LEVELS }],
    summary: (b) => label(LEVELS, b.level),
  },
  {
    type: 'sudoku', label: 'Sudoku', emoji: '🔢', group: G.fun, desc: 'Une grille à remplir au stylo',
    defaults: { difficulty: 'moyen' },
    fields: [
      { key: 'difficulty', label: 'Difficulté', kind: 'segmented', options: LEVELS },
      { key: 'seed', label: 'Numéro de grille', kind: 'number', optional: true, help: 'Laisse vide pour une nouvelle grille.' },
      { key: 'solution', label: 'Imprimer la solution de cette grille', kind: 'toggle' },
    ],
    summary: (b) => label(LEVELS, b.difficulty),
  },
  {
    type: 'word_search', label: 'Mots mêlés', emoji: '🔤', group: G.fun, desc: 'Des mots cachés dans une grille de lettres',
    defaults: { difficulty: 'moyen', theme: '', words: [] },
    fields: [
      { key: 'difficulty', label: 'Difficulté', kind: 'segmented', options: LEVELS },
      { key: 'theme', label: 'Thème', kind: 'select', options: WORD_THEMES },
      { key: 'words', label: 'Mes mots (remplacent le thème)', kind: 'list', placeholder: 'grand-mère…', add: 'Ajouter un mot' },
      { key: 'seed', label: 'Numéro de grille', kind: 'number', optional: true, help: 'Laisse vide pour une nouvelle grille.' },
      { key: 'solution', label: 'Imprimer la solution de cette grille', kind: 'toggle' },
    ],
    summary: (b) => [(b.words || []).length ? 'mes mots' : label(WORD_THEMES, b.theme || ''), label(LEVELS, b.difficulty)].join(' · '),
  },
  {
    type: 'maze', label: 'Labyrinthe', emoji: '🌀', group: G.fun, desc: 'Entrée en haut, sortie en bas',
    defaults: { width: 12, height: 16 },
    fields: [
      { key: 'width', label: 'Largeur', kind: 'number', min: 4, max: 40 },
      { key: 'height', label: 'Hauteur', kind: 'number', min: 4, max: 60 },
    ],
    summary: (b) => `${b.width} × ${b.height}`,
  },
  {
    type: 'news', label: 'Actualités', emoji: '📰', group: G.info, desc: 'Revue de presse depuis tes flux RSS',
    defaults: { title: 'France', feeds: ['https://www.franceinfo.fr/titres.rss', 'https://www.lemonde.fr/rss/une.xml'], count: 3, qr: 2, themes: [], exclude: [] },
    fields: [
      { key: 'title', label: 'Titre du bandeau', kind: 'text' },
      { key: 'feeds', label: 'Flux RSS', kind: 'list', placeholder: 'https://…', add: 'Ajouter un flux' },
      { key: 'themes', label: 'Thèmes favoris', kind: 'list', placeholder: 'climat, sciences…', add: 'Ajouter un thème' },
      { key: 'exclude', label: 'Mots à écarter', kind: 'list', placeholder: 'football…', add: 'Ajouter un mot' },
      { key: 'count', label: 'Articles', kind: 'number', min: 1, max: 5 },
      { key: 'qr', label: 'QR codes', kind: 'segmented', options: [[0, 'Aucun'], [1, '1'], [2, '2']] },
    ],
    summary: (b) => b.title,
  },
  {
    type: 'on_this_day', label: 'Éphéméride', emoji: '🏛️', group: G.info, desc: "C'est arrivé un jour comme aujourd'hui",
    defaults: { count: 3 },
    fields: [{ key: 'count', label: 'Événements', kind: 'number', min: 1, max: 10 }],
  },
  {
    type: 'crypto', label: 'Crypto', emoji: '🪙', group: G.info, desc: 'Cours et variation sur 24 h',
    defaults: { coins: ['bitcoin', 'ethereum'], currency: 'eur' },
    fields: [
      { key: 'coins', label: 'Cryptomonnaies (identifiants CoinGecko)', kind: 'list', placeholder: 'bitcoin', add: 'Ajouter' },
      { key: 'currency', label: 'Devise', kind: 'segmented', options: [['eur', 'Euro'], ['usd', 'Dollar']] },
    ],
    summary: (b) => (b.coins || []).join(', '),
  },
  {
    type: 'todo', label: 'À faire', emoji: '✅', group: G.org, desc: 'Une liste à cocher au stylo',
    defaults: { title: 'À faire', items: [''] },
    fields: [
      { key: 'title', label: 'Titre', kind: 'text' },
      { key: 'items', label: 'Choses à faire', kind: 'list', placeholder: 'Acheter du pain', add: 'Ajouter une ligne' },
    ],
    summary: (b) => `${(b.items || []).filter(Boolean).length} élément(s)`,
  },
];
const BLOCK = Object.fromEntries(BLOCKS.map((b) => [b.type, b]));
// Ancien nom accepté par le serveur.
BLOCK.enigme = BLOCK.riddle;
BLOCK.defi_sportif = BLOCK.workout;

const newBlock = (type) => ({ uid: uid(), open: true, data: { type, ...clone(BLOCK[type].defaults) } });

// Données d'un bloc telles qu'attendues par le serveur : champs vides retirés.
function cleanBlock(data) {
  const def = BLOCK[data.type];
  // Les paramètres sans champ dans le formulaire (ajoutés à la main dans le JSON, comme le
  // numéro d'une énigme) sont conservés tels quels : ouvrir puis enregistrer ne perd rien.
  const known = new Set((def?.fields || []).map((f) => f.key));
  const out = Object.fromEntries(Object.entries(data).filter(([k, v]) => !known.has(k) && v !== '' && v != null));
  out.type = data.type;
  for (const field of def?.fields || []) {
    let v = data[field.key];
    if (field.kind === 'list') v = (v || []).map((s) => String(s).trim()).filter(Boolean);
    if (field.kind === 'number' && v !== '' && v != null) v = Number(v);
    if (v === '' || v == null || Number.isNaN(v)) continue;
    if (field.kind === 'list' && v.length === 0 && field.key !== 'items') continue;
    out[field.key] = v;
  }
  return def?.normalize ? def.normalize(out) : out;
}

const toTicket = (state) => ({ cut: state.cut, spacing: Number(state.spacing), blocks: state.blocks.map((b) => cleanBlock(b.data)) });
const usesAi = (blocks) => blocks.some((b) => BLOCK[b.type || b.data?.type]?.ai);

function blockSummary(data) {
  const def = BLOCK[data.type];
  if (!def) return data.type;
  return def.summary ? def.summary(data) : def.desc;
}

const emptyComposer = () => ({ id: null, name: '', icon: '🧾', shared: false, schedules: [], cut: true, spacing: 1, blocks: [] });

function composerFromPreset(preset, copy = false) {
  const t = preset.ticket || {};
  return {
    id: copy ? null : preset.id,
    name: copy ? `${preset.name} (copie)` : preset.name,
    icon: preset.icon || '🧾',
    shared: copy ? false : !!preset.shared,
    schedules: copy ? [] : clone(preset.schedules || []),
    cut: t.cut ?? true,
    spacing: t.spacing ?? 1,
    blocks: (t.blocks || []).map((data) => ({ uid: uid(), open: false, data: { ...clone(BLOCK[data.type]?.defaults || {}), ...data } })),
  };
}

// ============================================================================
// Planification : jours et heures
// ============================================================================

const DAY_LETTERS = ['L', 'M', 'M', 'J', 'V', 'S', 'D'];
const DAY_NAMES = ['lun.', 'mar.', 'mer.', 'jeu.', 'ven.', 'sam.', 'dim.'];

function daysLabel(days) {
  const d = [...days].sort().join('');
  if (d === '1234567') return 'Tous les jours';
  if (d === '12345') return 'En semaine';
  if (d === '67') return 'Le week-end';
  return days.slice().sort().map((n) => DAY_NAMES[n - 1]).join(' ');
}
const timeLabel = (t) => t.replace(':', ' h ');
const scheduleLabel = (s) => `${daysLabel(s.days)} · ${timeLabel(s.time)}`;

// ============================================================================
// Composants de formulaire
// ============================================================================

function Field({ label: text, help, children }) {
  return html`<div class="field">${text && html`<label>${text}</label>`}${children}${help && html`<span class="help">${help}</span>`}</div>`;
}

function Switch({ label: text, checked, onChange, help }) {
  return html`<label class="switch">
    <span><span class="label">${text}</span>${help && html`<br/><span class="help muted" style="font-size:12.5px">${help}</span>`}</span>
    <input type="checkbox" checked=${!!checked} onChange=${(e) => onChange(e.target.checked)} />
    <span class="track"></span>
  </label>`;
}

function Segmented({ options, value, onChange }) {
  return html`<div class="segmented" role="radiogroup">
    ${options.map(([v, text]) => html`<button type="button" role="radio" aria-checked=${v === value}
      class=${v === value ? 'on' : ''} onClick=${() => onChange(v)}>${text}</button>`)}
  </div>`;
}

function ListEdit({ value, onChange, placeholder, add }) {
  const items = value?.length ? value : [''];
  const set = (i, v) => onChange(items.map((x, j) => (j === i ? v : x)));
  return html`<div class="listedit">
    ${items.map((item, i) => html`<div class="item" key=${i}>
      <input class="input" value=${item} placeholder=${placeholder} onInput=${(e) => set(i, e.target.value)}
        onKeyDown=${(e) => { if (e.key === 'Enter') { e.preventDefault(); onChange([...items, '']); } }} />
      <button type="button" class="iconbtn danger" aria-label="Retirer" onClick=${() => onChange(items.filter((_, j) => j !== i))}>
        <${Icon} name="x" /></button>
    </div>`)}
    <button type="button" class="btn ghost small" onClick=${() => onChange([...items, ''])}><${Icon} name="plus" size=${16} /> ${add || 'Ajouter'}</button>
  </div>`;
}

function PhotoField({ value, onChange }) {
  const input = useRef();
  const [busy, setBusy] = useState(false);
  const [local, setLocal] = useState(null);
  const pick = async (file) => {
    if (!file) return;
    setLocal(URL.createObjectURL(file));
    setBusy(true);
    try {
      onChange(await uploadPhoto(file));
    } catch (e) {
      toast(e.message, 'error');
      setLocal(null);
    } finally {
      setBusy(false);
    }
  };
  const src = local || (value ? `/api/images/${value}` : null);
  return html`<div class="photo-drop ${src ? 'has-photo' : ''}" onClick=${() => !src && input.current.click()} role="button" tabindex="0">
    <input ref=${input} type="file" accept="image/*" hidden onChange=${(e) => pick(e.target.files[0])} />
    ${src ? html`<img src=${src} alt="Photo choisie" />` : html`<div><div class="big-emoji">📷</div><strong>Prendre ou choisir une photo</strong></div>`}
    ${busy && html`<div style="position:absolute;inset:0;display:grid;place-items:center;background:rgb(0 0 0/35%);color:#fff"><${Spinner} /></div>`}
    ${src && !busy && html`<button type="button" class="iconbtn remove" aria-label="Retirer la photo"
      onClick=${(e) => { e.stopPropagation(); setLocal(null); onChange(null); }}><${Icon} name="x" /></button>`}
  </div>`;
}

function FieldInput({ field, value, onChange }) {
  switch (field.kind) {
    case 'textarea':
      return html`<textarea class="input" rows="3" value=${value ?? ''} placeholder=${field.placeholder} onInput=${(e) => onChange(e.target.value)}></textarea>`;
    case 'number':
      return html`<input class="input" type="number" inputmode="numeric" min=${field.min} max=${field.max} value=${value ?? ''}
        placeholder=${field.optional ? 'Au hasard' : ''} onInput=${(e) => onChange(e.target.value === '' ? '' : Number(e.target.value))} />`;
    case 'select':
      return html`<select class="input" value=${value ?? ''} onChange=${(e) => onChange(e.target.value)}>
        ${field.options.map(([v, text]) => html`<option value=${v}>${text}</option>`)}
      </select>`;
    case 'segmented':
      return html`<${Segmented} options=${field.options} value=${value} onChange=${onChange} />`;
    case 'icons':
      return html`<div class="pictos">${field.options.map(([v, emoji]) => html`<button type="button" class=${v === value ? 'on' : ''}
        aria-label=${v} onClick=${() => onChange(v)}>${emoji}</button>`)}</div>`;
    case 'list':
      return html`<${ListEdit} value=${value} onChange=${onChange} placeholder=${field.placeholder} add=${field.add} />`;
    case 'date':
      return html`<input class="input" type="date" value=${value ?? ''} onInput=${(e) => onChange(e.target.value)} />`;
    case 'image':
      return html`<${PhotoField} value=${value} onChange=${onChange} />`;
    default:
      return html`<input class="input" value=${value ?? ''} placeholder=${field.placeholder} onInput=${(e) => onChange(e.target.value)} />`;
  }
}

function BlockFields({ data, onChange }) {
  const def = BLOCK[data.type];
  if (!def?.fields.length) return html`<p class="muted first" style="margin-bottom:0">Rien à régler : ${def?.desc?.toLowerCase() || 'ce bloc se suffit à lui-même'}.</p>`;
  const set = (key) => (v) => onChange({ ...data, [key]: v });
  return html`<div class="first">${def.fields.map((f) => f.kind === 'toggle'
    ? html`<${Switch} key=${f.key} label=${f.label} help=${f.help} checked=${data[f.key]} onChange=${set(f.key)} />`
    : html`<${Field} key=${f.key} label=${f.label} help=${f.help}><${FieldInput} field=${f} value=${data[f.key]} onChange=${set(f.key)} /></${Field}>`)}
  </div>`;
}

// ============================================================================
// Fenêtres
// ============================================================================

function Sheet({ title, onClose, children, footer, wide }) {
  useEffect(() => {
    const onKey = (e) => e.key === 'Escape' && onClose();
    addEventListener('keydown', onKey);
    document.body.style.overflow = 'hidden';
    return () => { removeEventListener('keydown', onKey); document.body.style.overflow = ''; };
  }, []);
  return html`<div class="overlay" onClick=${(e) => e.target === e.currentTarget && onClose()}>
    <div class="sheet ${wide ? 'wide' : ''}" role="dialog" aria-modal="true" aria-label=${title}>
      <div class="grabber"></div>
      <div class="sheet-head"><h2>${title}</h2>
        <button class="iconbtn" aria-label="Fermer" onClick=${onClose}><${Icon} name="x" /></button></div>
      ${children}
      ${footer && html`<div class="sheet-foot">${footer}</div>`}
    </div>
  </div>`;
}

function Confirm({ title, message, action, danger, onConfirm, onClose }) {
  const [busy, setBusy] = useState(false);
  return html`<${Sheet} title=${title} onClose=${onClose} footer=${html`
    <button class="btn secondary" onClick=${onClose}>Annuler</button>
    <button class="btn ${danger ? 'danger' : 'primary'} grow" disabled=${busy}
      onClick=${async () => { setBusy(true); try { await onConfirm(); onClose(); } finally { setBusy(false); } }}>
      ${busy ? html`<${Spinner} />` : action}</button>`}>
    <p style="margin-top:0">${message}</p>
  </${Sheet}>`;
}

// ============================================================================
// Aperçu : ruban de papier
// ============================================================================

function Paper({ preview, loading }) {
  if (!preview) {
    return html`<div class="paper-wrap"><div class="paper ${loading ? 'loading' : ''}"><div class="paper-empty">
      ${loading ? html`<${Spinner} />` : 'Ajoute des blocs pour voir ton ticket ici.'}</div></div></div>`;
  }
  return html`<div class="paper-wrap"><div class="paper ${loading ? 'loading' : ''}">
    ${preview.ops.length === 0 && html`<div class="paper-empty">Ton ticket est vide.</div>`}
    ${preview.ops.map((op, i) => {
      if (op.t === 'feed') return html`<div class="feed" key=${i} style=${{ height: `${op.n * 1.32}em` }}></div>`;
      if (op.t === 'image') return html`<img class="img" key=${i} src=${op.src} alt="" style=${{ aspectRatio: `${op.width} / ${op.height}` }} />`;
      const cls = ['ln', op.bold && 'b', op.underline && 'u', op.reverse && 'r', op.small && 'small', op.flip && 'flip',
        op.align === 'center' && 'c', op.align === 'right' && 'rt'].filter(Boolean).join(' ');
      const size = op.size > 1 ? { fontSize: `${op.size * 100}%`, lineHeight: 1.18 } : null;
      return html`<div class=${cls} key=${i} style=${size}><span>${op.text}</span></div>`;
    })}
  </div></div>`;
}

function PaperMeta({ preview }) {
  if (!preview) return null;
  const cm = Math.round((preview.height_dots / 180) * 2.54);
  const failed = preview.reports.filter((r) => r.error).length;
  return html`<div class="paper-meta">
    <span>≈ ${cm} cm de papier</span>
    ${failed ? html`<span style="color:var(--danger)">${failed} bloc${failed > 1 ? 's' : ''} en erreur</span>` : html`<span>${preview.reports.length} bloc${preview.reports.length > 1 ? 's' : ''}</span>`}
  </div>`;
}

// Calcule l'aperçu d'un ticket, en différé pendant la saisie.
function usePreview(ticket) {
  const key = JSON.stringify(ticket);
  const debounced = useDebounced(key, 450);
  const [state, setState] = useState({ preview: null, loading: false });
  useEffect(() => {
    let alive = true;
    const t = JSON.parse(debounced);
    if (!t.blocks.length) { setState({ preview: null, loading: false }); return; }
    setState((s) => ({ ...s, loading: true }));
    api('POST', '/api/preview', { ticket: t })
      .then((preview) => alive && setState({ preview, loading: false }))
      .catch((e) => alive && setState((s) => ({ preview: s.preview, loading: false, error: e.message })));
    return () => { alive = false; };
  }, [debounced]);
  return { ...state, loading: state.loading || key !== debounced };
}

// ============================================================================
// Écran : connexion
// ============================================================================

function Login({ onLogin }) {
  const [users, setUsers] = useState(null);
  const [who, setWho] = useState(store.get('printr.lastUser'));
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [shake, setShake] = useState(false);
  const field = useRef();

  useEffect(() => { api('GET', '/api/users').then(setUsers).catch(() => setUsers([])); }, []);
  useEffect(() => { if (who) field.current?.focus(); }, [who]);

  const submit = async (e) => {
    e.preventDefault();
    setBusy(true);
    try {
      const { user } = await api('POST', '/api/login', { user_id: who, password });
      store.set('printr.lastUser', user.id);
      onLogin(user);
    } catch (err) {
      toast(err.message, 'error');
      setShake(true);
      setTimeout(() => setShake(false), 450);
      setPassword('');
    } finally {
      setBusy(false);
    }
  };

  return html`<div class="login"><div class="login-card">
    <img class="logo" src="/mark.png" alt="" />
    <h1>Printr</h1>
    <p class="muted" style="margin:0">L'imprimante de la maison</p>
    ${users === null ? html`<div style="margin:32px"><${Spinner} /></div>` : users.length === 0
      ? html`<div class="card" style="padding:18px;margin-top:28px;text-align:left">
          <strong>Aucun compte pour l'instant.</strong>
          <p class="muted" style="margin:6px 0 0">Crée-en un sur le serveur : <code>printr user add Prénom</code></p></div>`
      : html`<div class="who">${users.map((u) => html`<button type="button" class=${who === u.id ? 'on' : ''}
          style=${{ color: u.color }} onClick=${() => setWho(u.id)}><${Avatar} user=${u} size="large" />
          <span style="color:var(--ink-2)">${u.name}</span></button>`)}</div>`}
    ${who && users?.length > 0 && html`<form onSubmit=${submit} class=${shake ? 'shake' : ''}>
      <div class="field"><input ref=${field} class="input" type="password" autocomplete="current-password"
        placeholder="Mot de passe" value=${password} onInput=${(e) => setPassword(e.target.value)}
        style="text-align:center;font-size:18px;min-height:52px" /></div>
      <button class="btn primary big block" disabled=${busy || !password}>${busy ? html`<${Spinner} />` : 'Entrer'}</button>
    </form>`}
  </div></div>`;
}

// ============================================================================
// Écran : accueil (presets)
// ============================================================================

function PresetCard({ preset, me, onPrint, onEdit, onCopy, onDelete }) {
  const [busy, setBusy] = useState(false);
  const mine = preset.owner?.id === me.id;
  const blocks = preset.ticket?.blocks || [];
  const print = async () => { setBusy(true); try { await onPrint(preset); } finally { setBusy(false); } };
  return html`<div class="card preset">
    <div class="preset-head">
      <div class="preset-icon">${preset.icon || '🧾'}</div>
      <div class="grow">
        <h3>${preset.name}</h3>
        <div class="meta">
          ${preset.schedules?.filter((s) => s.enabled).map((s) => html`<span class="chip accent" key=${s.id}><${Icon} name="clock" />${scheduleLabel(s)}</span>`)}
          ${preset.shared && mine && html`<span class="chip"><${Icon} name="users" />Partagé</span>`}
          ${!mine && preset.owner && html`<span class="chip"><${Avatar} user=${preset.owner} size="small" />${preset.owner.name}</span>`}
        </div>
      </div>
    </div>
    <div class="blocks-line">${blocks.map((b) => `${BLOCK[b.type]?.emoji || '▫️'} ${BLOCK[b.type]?.label || b.type}`).join('  ·  ') || 'Ticket vide'}</div>
    <div class="preset-actions">
      <button class="btn primary small" disabled=${busy} onClick=${print}>${busy ? html`<${Spinner} />` : html`<${Icon} name="printer" size=${17} />`} Imprimer</button>
      ${mine
        ? html`<button class="btn secondary small" onClick=${() => onEdit(preset)}><${Icon} name="edit" size=${16} /> Modifier</button>`
        : html`<button class="btn secondary small" onClick=${() => onCopy(preset)}><${Icon} name="copy" size=${16} /> Copier</button>`}
      <span class="grow"></span>
      ${mine && html`<button class="iconbtn" aria-label="Dupliquer" title="Dupliquer" onClick=${() => onCopy(preset)}><${Icon} name="copy" /></button>
        <button class="iconbtn danger" aria-label="Supprimer" title="Supprimer" onClick=${() => onDelete(preset)}><${Icon} name="trash" /></button>`}
    </div>
  </div>`;
}

function Home({ me, presets, reload, openComposer }) {
  const [confirm, setConfirm] = useState(null);
  const mine = presets?.filter((p) => p.owner?.id === me.id) || [];
  const family = presets?.filter((p) => p.owner?.id !== me.id) || [];
  const hour = new Date().getHours();
  const hello = hour < 5 ? 'Bonne nuit' : hour < 18 ? 'Bonjour' : 'Bonsoir';
  const today = new Date().toLocaleDateString('fr-FR', { weekday: 'long', day: 'numeric', month: 'long' });

  const print = async (preset) => {
    try {
      const r = await api('POST', `/api/presets/${preset.id}/print`);
      r.errors?.length ? toast(`Imprimé, avec ${r.errors.length} bloc(s) en erreur`, 'error', '⚠️') : toast('Imprimé !', 'ok', '🧾');
    } catch (e) { toast(e.message, 'error'); }
  };
  const remove = (preset) => setConfirm({
    title: 'Supprimer ce ticket ?',
    message: html`« ${preset.name} » et ses planifications seront supprimés définitivement.`,
    action: 'Supprimer', danger: true,
    onConfirm: async () => { await api('DELETE', `/api/presets/${preset.id}`); toast('Ticket supprimé', 'ok', '🗑️'); reload(); },
  });
  const cards = (list) => html`<div class="presets">${list.map((p) => html`<${PresetCard} key=${p.id} preset=${p} me=${me}
    onPrint=${print} onEdit=${(x) => openComposer(composerFromPreset(x))} onCopy=${(x) => openComposer(composerFromPreset(x, true))} onDelete=${remove} />`)}</div>`;

  return html`<div class="page">
    <h2 style="margin:4px 2px 2px;font-size:26px;letter-spacing:-0.02em">${hello} ${me.name} 👋</h2>
    <p class="muted" style="margin:0 2px 18px">${capitalize(today)}</p>
    <div class="hero">
      <a class="action-card coral" href="#/message"><div class="big-emoji">💌</div><div><strong>Petit mot</strong><span>Une photo, quelques mots</span></div></a>
      <button class="action-card sky" onClick=${() => openComposer(emptyComposer())}><div class="big-emoji">🧾</div><div><strong>Nouveau ticket</strong><span>Compose bloc par bloc</span></div></button>
    </div>
    <div class="section-title">Mes tickets</div>
    ${presets === null ? html`<div style="padding:30px;display:grid;place-items:center"><${Spinner} /></div>`
      : mine.length ? cards(mine)
      : html`<div class="empty"><div class="big-emoji">🌱</div><h3>Pas encore de ticket enregistré</h3>
          <p class="muted" style="margin:0 0 16px">Compose un ticket, puis enregistre-le pour l'imprimer en un geste… ou chaque matin.</p>
          <button class="btn primary" onClick=${() => openComposer(emptyComposer())}><${Icon} name="plus" /> Composer mon premier ticket</button></div>`}
    ${family.length > 0 && html`<div class="section-title">Partagés par la famille</div>${cards(family)}`}
    ${confirm && html`<${Confirm} ...${confirm} onClose=${() => setConfirm(null)} />`}
  </div>`;
}

// ============================================================================
// Écran : composer
// ============================================================================

function Catalog({ onPick, onClose }) {
  const [query, setQuery] = useState('');
  const q = query.trim().toLowerCase();
  const matches = BLOCKS.filter((b) => !q || `${b.label} ${b.desc}`.toLowerCase().includes(q));
  const groups = [...new Set(matches.map((b) => b.group))];
  return html`<${Sheet} title="Ajouter un bloc" onClose=${onClose} wide>
    <div class="catalog-search"><input class="input" placeholder="Rechercher : météo, photo, sudoku…" value=${query}
      onInput=${(e) => setQuery(e.target.value)} autofocus /></div>
    ${groups.map((g) => html`<div class="catalog-group" key=${g}><h4>${g}</h4><div class="catalog">
      ${matches.filter((b) => b.group === g).map((b) => html`<button type="button" key=${b.type} onClick=${() => onPick(b.type)}>
        <span class="row" style="width:100%"><span class="emoji">${b.emoji}</span><span class="grow"></span>${b.ai && html`<span class="badge">Claude</span>`}</span>
        <strong>${b.label}</strong><span>${b.desc}</span></button>`)}
    </div></div>`)}
    ${matches.length === 0 && html`<p class="muted" style="text-align:center;padding:24px">Aucun bloc ne correspond.</p>`}
  </${Sheet}>`;
}

function BlockCard({ item, index, count, error, onChange, onToggle, onMove, onCopy, onRemove }) {
  const def = BLOCK[item.data.type] || { emoji: '▫️', label: item.data.type };
  return html`<div class="card block ${item.open ? 'open' : ''} ${error ? 'error' : ''}">
    <div class="block-head" onClick=${onToggle}>
      <div class="block-emoji">${def.emoji}</div>
      <div class="grow">
        <div class="block-title">${def.label}${def.ai ? html` <span class="chip warn" style="height:20px;font-size:11px;margin-left:4px">Claude</span>` : ''}</div>
        <div class="block-summary">${blockSummary(item.data)}</div>
      </div>
      <div class="block-tools" onClick=${(e) => e.stopPropagation()}>
        <button class="iconbtn" aria-label="Monter" disabled=${index === 0} onClick=${() => onMove(-1)}><${Icon} name="up" /></button>
        <button class="iconbtn" aria-label="Descendre" disabled=${index === count - 1} onClick=${() => onMove(1)}><${Icon} name="down" /></button>
        <button class="iconbtn" aria-label="Dupliquer" onClick=${onCopy}><${Icon} name="copy" /></button>
        <button class="iconbtn danger" aria-label="Supprimer" onClick=${onRemove}><${Icon} name="trash" /></button>
      </div>
    </div>
    ${error && html`<div class="block-error">${error}</div>`}
    ${item.open && html`<div class="block-body"><${BlockFields} data=${item.data} onChange=${onChange} /></div>`}
  </div>`;
}

const ICON_CHOICES = ['🧾', '☀️', '🌙', '🔮', '💌', '❤️', '📰', '🧩', '💪', '🌤️', '✅', '🎉', '☕', '🏠', '🎂', '🌸'];

// Heure au format 24 h, quel que soit l'appareil : le champ <input type="time"> s'affiche
// en 12 h selon la langue du système, et un « AM » oublié décale l'impression de 12 heures.
function TimePicker({ value, onChange }) {
  const [h, m] = (value || '08:00').split(':');
  const pad = (n) => String(n).padStart(2, '0');
  const minutes = Array.from({ length: 12 }, (_, i) => pad(i * 5));
  if (!minutes.includes(m)) minutes.push(m);
  minutes.sort();
  return html`<div class="time-picker" role="group" aria-label="Heure">
    <select aria-label="Heures" value=${h} onChange=${(e) => onChange(`${e.target.value}:${m}`)}>
      ${Array.from({ length: 24 }, (_, i) => pad(i)).map((v) => html`<option value=${v}>${v}</option>`)}
    </select>
    <span>h</span>
    <select aria-label="Minutes" value=${m} onChange=${(e) => onChange(`${h}:${e.target.value}`)}>
      ${minutes.map((v) => html`<option value=${v}>${v}</option>`)}
    </select>
  </div>`;
}

function ScheduleEditor({ schedules, onChange }) {
  const update = (i, patch) => onChange(schedules.map((s, j) => (j === i ? { ...s, ...patch } : s)));
  const toggleDay = (i, d) => {
    const days = schedules[i].days.includes(d) ? schedules[i].days.filter((x) => x !== d) : [...schedules[i].days, d];
    update(i, { days: days.sort() });
  };
  return html`<div>
    ${schedules.map((s, i) => html`<div class="schedule" key=${s.id || i}>
      <div class="row wrap" style="justify-content:space-between">
        <div class="days">${DAY_LETTERS.map((l, k) => html`<button type="button" class=${s.days.includes(k + 1) ? 'on' : ''}
          aria-label=${DAY_NAMES[k]} onClick=${() => toggleDay(i, k + 1)}>${l}</button>`)}</div>
        <${TimePicker} value=${s.time} onChange=${(time) => update(i, { time })} />
      </div>
      <div class="row" style="margin-top:8px">
        <div class="quick-days">
          <button type="button" class="btn ghost small" onClick=${() => update(i, { days: [1, 2, 3, 4, 5, 6, 7] })}>Tous les jours</button>
          <button type="button" class="btn ghost small" onClick=${() => update(i, { days: [1, 2, 3, 4, 5] })}>En semaine</button>
          <button type="button" class="btn ghost small" onClick=${() => update(i, { days: [6, 7] })}>Week-end</button>
        </div>
        <span class="grow"></span>
        <label class="switch" style="padding:0"><input type="checkbox" checked=${s.enabled} onChange=${(e) => update(i, { enabled: e.target.checked })} /><span class="track"></span></label>
        <button class="iconbtn danger" aria-label="Retirer" onClick=${() => onChange(schedules.filter((_, j) => j !== i))}><${Icon} name="trash" /></button>
      </div>
    </div>`)}
    <button type="button" class="btn secondary small" onClick=${() => onChange([...schedules, { days: [1, 2, 3, 4, 5], time: '08:00', enabled: true }])}>
      <${Icon} name="clock" size=${16} /> Ajouter un horaire</button>
  </div>`;
}

function SaveSheet({ state, onSave, onClose, focusSchedule }) {
  const [draft, setDraft] = useState({ name: state.name, icon: state.icon, shared: state.shared, schedules: clone(state.schedules) });
  const [busy, setBusy] = useState(false);
  const set = (patch) => setDraft((d) => ({ ...d, ...patch }));
  const invalid = draft.schedules.some((s) => !s.days.length || !s.time);
  const ai = usesAi(state.blocks.map((b) => b.data));
  const save = async () => { setBusy(true); try { await onSave(draft); onClose(); } catch (e) { toast(e.message, 'error'); } finally { setBusy(false); } };
  return html`<${Sheet} title=${state.id ? 'Réglages du ticket' : 'Enregistrer le ticket'} onClose=${onClose} footer=${html`
    <button class="btn secondary" onClick=${onClose}>Annuler</button>
    <button class="btn primary grow" disabled=${busy || !draft.name.trim() || invalid} onClick=${save}>${busy ? html`<${Spinner} />` : 'Enregistrer'}</button>`}>
    <${Field} label="Nom"><input class="input" value=${draft.name} placeholder="Horoscope du matin" autofocus=${!focusSchedule}
      onInput=${(e) => set({ name: e.target.value })} /></${Field}>
    <${Field} label="Icône"><div class="icon-picker">${ICON_CHOICES.map((i) => html`<button type="button" class=${draft.icon === i ? 'on' : ''}
      onClick=${() => set({ icon: i })}>${i}</button>`)}</div></${Field}>
    <${Switch} label="Partager avec la famille" help="Les autres pourront l'imprimer et le copier." checked=${draft.shared} onChange=${(v) => set({ shared: v })} />
    <div class="section-title" style="margin-top:18px">Impression automatique</div>
    <${ScheduleEditor} schedules=${draft.schedules} onChange=${(schedules) => set({ schedules })} />
    ${ai && draft.schedules.length > 0 && html`<p class="help muted" style="font-size:13px;margin-bottom:0">
      🔮 Ce ticket utilise Claude : un petit appel payant par impression (le contenu est gardé en cache pour la journée).</p>`}
  </${Sheet}>`;
}

function Composer({ initial, onSaved }) {
  const [state, setState] = useState(initial);
  const [catalog, setCatalog] = useState(false);
  const [saveSheet, setSaveSheet] = useState(null);
  const [mobilePreview, setMobilePreview] = useState(false);
  const [printing, setPrinting] = useState(false);
  const [saving, setSaving] = useState(false);
  const ticket = useMemo(() => toTicket(state), [state]);
  const { preview, loading } = usePreview(ticket);

  useEffect(() => setState(initial), [initial]);
  // Brouillon gardé localement tant qu'il n'est pas enregistré.
  useEffect(() => { if (!state.id) store.set('printr.draft', state); }, [state]);

  const setBlocks = (fn) => setState((s) => ({ ...s, blocks: fn(s.blocks) }));
  const add = (type) => { setBlocks((b) => [...b.map((x) => ({ ...x, open: false })), newBlock(type)]); setCatalog(false); };
  const move = (i, d) => setBlocks((b) => { const n = [...b]; [n[i], n[i + d]] = [n[i + d], n[i]]; return n; });
  const errors = preview?.reports?.map((r) => r.error) || [];

  const print = async () => {
    setPrinting(true);
    try {
      const r = await api('POST', '/api/print', { ticket, label: state.name || 'Ticket composé' });
      r.errors?.length ? toast(`Imprimé, avec ${r.errors.length} bloc(s) en erreur`, 'error', '⚠️') : toast('Imprimé !', 'ok', '🧾');
    } catch (e) { toast(e.message, 'error'); } finally { setPrinting(false); }
  };

  const persist = async (meta) => {
    const body = { name: meta.name, icon: meta.icon, shared: meta.shared, schedules: meta.schedules, ticket };
    const saved = state.id ? await api('PUT', `/api/presets/${state.id}`, body) : await api('POST', '/api/presets', body);
    if (!state.id) store.del('printr.draft');
    setState((s) => ({ ...s, ...meta, id: saved.id, schedules: saved.schedules }));
    toast(state.id ? 'Ticket mis à jour' : 'Ticket enregistré', 'ok', '✨');
    onSaved();
  };
  const quickSave = async () => {
    if (!state.id) return setSaveSheet({});
    setSaving(true);
    try { await persist(state); } catch (e) { toast(e.message, 'error'); } finally { setSaving(false); }
  };

  return html`<div class="page">
    <div class="composer">
      <div>
        ${state.id && html`<div class="row" style="margin:0 2px 14px">
          <span style="font-size:26px">${state.icon}</span>
          <div class="grow"><div style="font-weight:800;font-size:18px">${state.name}</div>
            <div class="muted" style="font-size:13px">${state.schedules.filter((s) => s.enabled).map(scheduleLabel).join(' · ') || 'Pas de planification'}</div></div>
          <button class="btn secondary small" onClick=${() => setSaveSheet({ focusSchedule: true })}><${Icon} name="sliders" size=${16} /> Réglages</button>
        </div>`}
        <div class="blocklist">
          ${state.blocks.length === 0 && html`<div class="empty" style="padding:30px 18px"><div class="big-emoji">✨</div>
            <h3>Compose ton ticket</h3><p class="muted" style="margin:0 0 14px">Quelques idées pour commencer :</p>
            <div class="row wrap" style="justify-content:center">${['date', 'weather', 'horoscope', 'riddle', 'workout', 'news'].map((t) =>
              html`<button class="btn secondary small" onClick=${() => add(t)}>${BLOCK[t].emoji} ${BLOCK[t].label}</button>`)}</div></div>`}
          ${state.blocks.map((item, i) => html`<${BlockCard} key=${item.uid} item=${item} index=${i} count=${state.blocks.length} error=${errors[i]}
            onToggle=${() => setBlocks((b) => b.map((x) => (x.uid === item.uid ? { ...x, open: !x.open } : x)))}
            onChange=${(data) => setBlocks((b) => b.map((x) => (x.uid === item.uid ? { ...x, data } : x)))}
            onMove=${(d) => move(i, d)}
            onCopy=${() => setBlocks((b) => [...b.slice(0, i + 1), { uid: uid(), open: true, data: clone(item.data) }, ...b.slice(i + 1)])}
            onRemove=${() => setBlocks((b) => b.filter((x) => x.uid !== item.uid))} />`)}
          <button class="add-block" onClick=${() => setCatalog(true)}><${Icon} name="plus" /> Ajouter un bloc</button>
        </div>
        <div class="card ticket-settings">
          <${Switch} label="Couper le papier à la fin" checked=${state.cut} onChange=${(cut) => setState((s) => ({ ...s, cut }))} />
          <div class="spacing-row"><span class="label">Espace entre les blocs</span>
            <${Segmented} options=${[[0, 'Aucun'], [1, '1 ligne'], [2, '2 lignes']]} value=${Number(state.spacing)} onChange=${(spacing) => setState((s) => ({ ...s, spacing }))} /></div>
        </div>
        <div class="actionbar">
          <button class="btn ghost preview-fab" aria-label="Aperçu" onClick=${() => setMobilePreview(true)}><${Icon} name="eye" /><span class="lbl">Aperçu</span></button>
          <button class="btn secondary" aria-label=${state.id ? 'Enregistrer' : 'Garder'} disabled=${saving || !state.blocks.length} onClick=${quickSave}>${saving ? html`<${Spinner} />` : html`<${Icon} name="save" />`}<span class="lbl">${state.id ? 'Enregistrer' : 'Garder'}</span></button>
          <button class="btn primary" disabled=${printing || !state.blocks.length} onClick=${print}>${printing ? html`<${Spinner} />` : html`<${Icon} name="printer" />`} Imprimer</button>
        </div>
      </div>
      <aside class="preview-col"><${Paper} preview=${preview} loading=${loading} /><${PaperMeta} preview=${preview} /></aside>
    </div>
    ${catalog && html`<${Catalog} onPick=${add} onClose=${() => setCatalog(false)} />`}
    ${saveSheet && html`<${SaveSheet} state=${state} focusSchedule=${saveSheet.focusSchedule} onSave=${persist} onClose=${() => setSaveSheet(null)} />`}
    ${mobilePreview && html`<${Sheet} title="Aperçu" onClose=${() => setMobilePreview(false)} footer=${html`
      <button class="btn primary grow" disabled=${printing} onClick=${print}>${printing ? html`<${Spinner} />` : html`<${Icon} name="printer" />`} Imprimer</button>`}>
      <${Paper} preview=${preview} loading=${loading} /><${PaperMeta} preview=${preview} /></${Sheet}>`}
  </div>`;
}

// ============================================================================
// Écran : petit mot
// ============================================================================

function Message({ me, onSaved }) {
  const [title, setTitle] = useState('Pour toi');
  const [text, setText] = useState('');
  const [picto, setPicto] = useState('coeur');
  const [photo, setPhoto] = useState(null);
  const [signature, setSignature] = useState(me.name);
  const [printing, setPrinting] = useState(false);
  const [saveSheet, setSaveSheet] = useState(false);
  const [done, setDone] = useState(false);

  const ticket = useMemo(() => {
    const blocks = [];
    if (picto) blocks.push({ type: 'picto', shape: picto, size: 'moyen', count: 1 });
    if (title.trim()) blocks.push({ type: 'title', text: title.trim(), size: 2 });
    if (photo) blocks.push({ type: 'image', upload: photo, dither: true });
    if (text.trim()) blocks.push({ type: 'text', text: text.trim(), align: 'center' });
    if (signature.trim()) blocks.push({ type: 'text', text: `- ${signature.trim()}`, align: 'right', bold: true });
    blocks.push({ type: 'date' });
    return { cut: true, spacing: 1, blocks };
  }, [title, text, picto, photo, signature]);
  const { preview, loading } = usePreview(ticket);
  const empty = !text.trim() && !photo;

  const print = async () => {
    setPrinting(true);
    try {
      await api('POST', '/api/print', { ticket, label: `Petit mot${title.trim() ? ` · ${title.trim()}` : ''}` });
      setDone(true);
      toast('Envoyé à l’imprimante !', 'ok', '💌');
      setTimeout(() => setDone(false), 2400);
    } catch (e) { toast(e.message, 'error'); } finally { setPrinting(false); }
  };

  return html`<div class="page"><div class="composer">
    <div class="message">
      <${Field} label="Une photo ?"><${PhotoField} value=${photo} onChange=${setPhoto} /></${Field}>
      <${Field} label="Un dessin ?"><div class="pictos">
        <button type="button" class=${!picto ? 'on' : ''} aria-label="Aucun" onClick=${() => setPicto(null)} style="font-size:15px;font-weight:800">Ø</button>
        ${PICTOS.map(([v, e]) => html`<button type="button" class=${picto === v ? 'on' : ''} aria-label=${v} onClick=${() => setPicto(v)}>${e}</button>`)}
      </div></${Field}>
      <${Field} label="Titre"><input class="input" value=${title} placeholder="Pour toi" onInput=${(e) => setTitle(e.target.value)} /></${Field}>
      <${Field} label="Ton petit mot"><textarea class="input" rows="5" value=${text} placeholder="Je pense fort à toi…"
        onInput=${(e) => setText(e.target.value)}></textarea></${Field}>
      <${Field} label="Signé"><input class="input" value=${signature} onInput=${(e) => setSignature(e.target.value)} /></${Field}>
      <button class="btn primary big block" disabled=${printing || empty} onClick=${print}>
        ${printing ? html`<${Spinner} />` : done ? html`<${Icon} name="check" /> Envoyé !` : html`<${Icon} name="send" /> Envoyer à l'imprimante`}</button>
      <button class="btn ghost block" style="margin-top:8px" disabled=${empty} onClick=${() => setSaveSheet(true)}><${Icon} name="save" /> Garder comme modèle</button>
      <div class="section-title" style="display:block">Aperçu</div>
      <div style="display:block" class="message-preview"><${Paper} preview=${preview} loading=${loading} /></div>
    </div>
    <aside class="preview-col"><${Paper} preview=${preview} loading=${loading} /><${PaperMeta} preview=${preview} /></aside>
    ${saveSheet && html`<${SaveSheet}
      state=${{ ...emptyComposer(), name: title.trim() || 'Petit mot', icon: '💌', blocks: ticket.blocks.map((data) => ({ data })) }}
      onSave=${async (meta) => { await api('POST', '/api/presets', { ...meta, ticket }); toast('Modèle enregistré', 'ok', '✨'); onSaved(); }}
      onClose=${() => setSaveSheet(false)} />`}
  </div></div>`;
}

// ============================================================================
// Écran : historique
// ============================================================================

function History({ users }) {
  const [entries, setEntries] = useState(null);
  useEffect(() => { api('GET', '/api/history').then(setEntries).catch((e) => { toast(e.message, 'error'); setEntries([]); }); }, []);
  const byName = Object.fromEntries((users || []).map((u) => [u.name, u]));
  const dayLabel = (d) => {
    const today = new Date(); const y = new Date(Date.now() - 864e5);
    if (d.toDateString() === today.toDateString()) return "Aujourd'hui";
    if (d.toDateString() === y.toDateString()) return 'Hier';
    return capitalize(d.toLocaleDateString('fr-FR', { weekday: 'long', day: 'numeric', month: 'long' }));
  };
  if (entries === null) return html`<div class="page" style="display:grid;place-items:center;min-height:40vh"><${Spinner} /></div>`;
  if (!entries.length) return html`<div class="page"><div class="empty"><div class="big-emoji">🕰️</div><h3>Rien d'imprimé pour l'instant</h3>
    <p class="muted" style="margin:0">Les impressions de toute la famille apparaîtront ici.</p></div></div>`;
  const groups = [];
  for (const e of entries) {
    const label = dayLabel(new Date(e.at));
    if (groups.at(-1)?.label !== label) groups.push({ label, items: [] });
    groups.at(-1).items.push(e);
  }
  return html`<div class="page" style="max-width:760px">
    ${groups.map((g) => html`<div key=${g.label}><div class="history-day">${g.label}</div><div class="card">
      ${g.items.map((e, i) => html`<div class="history-item" key=${i}>
        <div class="history-time">${new Date(e.at).toLocaleTimeString('fr-FR', { hour: '2-digit', minute: '2-digit' })}</div>
        ${byName[e.by] ? html`<${Avatar} user=${byName[e.by]} size="small" />` : html`<span class="chip" style="height:22px;padding:0 6px"><${Icon} name="clock" /></span>`}
        <div class="grow"><strong>${e.label}</strong>
          <div class="muted" style="font-size:13px">${e.by === 'planification' ? 'Impression planifiée' : `par ${e.by}`}</div>
          ${e.errors?.length > 0 && html`<ul class="history-errors">${e.errors.map((err) => html`<li>${err}</li>`)}</ul>`}</div>
        <span class="chip ${e.ok ? (e.errors?.length ? 'warn' : 'ok') : 'danger'}">${e.ok ? (e.errors?.length ? 'Partiel' : 'Imprimé') : 'Échec'}</span>
      </div>`)}
    </div></div>`)}
  </div>`;
}

// ============================================================================
// Application
// ============================================================================

function Toasts() {
  const [items, setItems] = useState([]);
  useEffect(() => bus.on('toast', (t) => {
    const id = uid();
    setItems((x) => [...x.slice(-2), { ...t, id }]);
    setTimeout(() => setItems((x) => x.filter((i) => i.id !== id)), t.kind === 'error' ? 5000 : 2600);
  }), []);
  return html`<div class="toasts" aria-live="polite">${items.map((t) => html`<div class="toast ${t.kind}" key=${t.id}>
    <span class="emoji">${t.emoji || (t.kind === 'error' ? '⚠️' : '✓')}</span><span>${t.message}</span></div>`)}</div>`;
}

const NAV = [
  { hash: '#/', label: 'Accueil', icon: 'home', title: 'Printr' },
  { hash: '#/compose', label: 'Composer', icon: 'ticket', title: 'Composer' },
  { hash: '#/message', label: 'Petit mot', icon: 'heart', title: 'Un petit mot' },
  { hash: '#/history', label: 'Historique', icon: 'history', title: 'Historique' },
];

function App() {
  const [me, setMe] = useState(undefined);
  const [users, setUsers] = useState([]);
  const [presets, setPresets] = useState(null);
  const [composer, setComposer] = useState(() => store.get('printr.draft', null) || emptyComposer());
  const [scrolled, setScrolled] = useState(false);
  const [menu, setMenu] = useState(false);
  const hash = useHash();

  const loadPresets = useCallback(() => api('GET', '/api/presets').then(setPresets).catch((e) => toast(e.message, 'error')), []);

  useEffect(() => {
    api('GET', '/api/me').then(({ user }) => setMe(user)).catch(() => setMe(null));
    return bus.on('unauthorized', () => setMe(null));
  }, []);
  useEffect(() => {
    if (!me) return;
    loadPresets();
    api('GET', '/api/users').then(setUsers).catch(() => {});
  }, [me]);
  useEffect(() => {
    const onScroll = () => setScrolled(scrollY > 4);
    addEventListener('scroll', onScroll, { passive: true });
    return () => removeEventListener('scroll', onScroll);
  }, []);

  if (me === undefined) return html`<div class="boot"><${Spinner} /></div><${Toasts} />`;
  if (me === null) return html`<${Login} onLogin=${setMe} /><${Toasts} />`;

  const openComposer = (state) => { setComposer(state); location.hash = '#/compose'; };
  const logout = async () => { await api('POST', '/api/logout').catch(() => {}); setMenu(false); setMe(null); };
  const route = NAV.find((n) => n.hash === hash) || NAV[0];
  const title = route.hash === '#/compose' ? (composer.id ? 'Modifier le ticket' : 'Nouveau ticket') : route.title;

  let page;
  if (route.hash === '#/compose') page = html`<${Composer} initial=${composer} onSaved=${loadPresets} />`;
  else if (route.hash === '#/message') page = html`<${Message} me=${me} onSaved=${loadPresets} />`;
  else if (route.hash === '#/history') page = html`<${History} users=${users} />`;
  else page = html`<${Home} me=${me} presets=${presets} reload=${loadPresets} openComposer=${openComposer} />`;

  return html`<div class="shell">
    <nav class="sidebar" aria-label="Navigation">
      <div class="brand"><img src="/mark.png" alt="" />Printr</div>
      ${NAV.map((n) => html`<a class="navlink ${n.hash === route.hash ? 'active' : ''}" href=${n.hash}
        onClick=${n.hash === '#/compose' ? (e) => { if (route.hash !== '#/compose') { e.preventDefault(); openComposer(store.get('printr.draft', null) || emptyComposer()); } } : undefined}>
        <${Icon} name=${n.icon} />${n.label}</a>`)}
      <div class="spacer"></div>
      <div class="navlink" style="cursor:default"><${Avatar} user=${me} /><span class="grow">${me.name}</span>
        <button class="iconbtn" aria-label="Se déconnecter" title="Se déconnecter" onClick=${logout}><${Icon} name="logout" /></button></div>
    </nav>
    <main class="main">
      <header class="topbar ${scrolled ? 'scrolled' : ''}">
        <h1>${title}</h1>
        ${route.hash === '#/compose' && composer.id && html`<button class="btn ghost small" onClick=${() => openComposer(emptyComposer())}><${Icon} name="plus" size=${16} /> Nouveau</button>`}
        <button class="iconbtn me" style="width:auto;height:auto;padding:2px;border-radius:50%" aria-label="Mon compte" onClick=${() => setMenu(true)}><${Avatar} user=${me} /></button>
      </header>
      ${page}
    </main>
    <nav class="tabbar" aria-label="Navigation">
      ${NAV.map((n) => html`<a class="tab ${n.hash === route.hash ? 'active' : ''}" href=${n.hash}><${Icon} name=${n.icon} />${n.label}</a>`)}
    </nav>
    ${menu && html`<${Sheet} title=${me.name} onClose=${() => setMenu(false)}>
      <div class="row" style="margin-bottom:14px"><${Avatar} user=${me} size="large" /><div><strong>${me.name}</strong>
        <div class="muted">Connecté sur cet appareil</div></div></div>
      <button class="btn danger block" onClick=${logout}><${Icon} name="logout" /> Se déconnecter</button>
    </${Sheet}>`}
    <${Toasts} />
  </div>`;
}

render(html`<${App} />`, document.getElementById('app'));
