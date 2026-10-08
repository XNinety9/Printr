// Surligne, dans la colonne « Sur cette page », la section en cours de lecture (repris d'Otter).
(() => {
  const toc = document.querySelector(".toc");
  if (!toc) return;
  const links = new Map(
    [...toc.querySelectorAll("a[href^='#']")].map((a) => [decodeURIComponent(a.hash.slice(1)), a]),
  );
  const headings = [...document.querySelectorAll("article.doc h2[id], article.doc h3[id]")].filter((h) =>
    links.has(h.id),
  );
  if (!headings.length) return;
  let active = null;

  function update() {
    // La section dont le titre est passé en dernier sous la barre de navigation ; la dernière
    // en bas de page, où les sections courtes n'atteignent jamais le haut.
    const line = document.querySelector(".nav").offsetHeight + 24;
    let current = headings[0];
    for (const h of headings) {
      if (h.getBoundingClientRect().top > line) break;
      current = h;
    }
    if (innerHeight + scrollY >= document.documentElement.scrollHeight - 4) current = headings.at(-1);
    const link = links.get(current.id);
    if (link === active) return;
    active?.classList.remove("active");
    toc.querySelectorAll(".parent").forEach((a) => a.classList.remove("parent"));
    link.classList.add("active");
    // Une sous-section marque aussi sa section.
    link.parentElement.parentElement.closest("li")?.querySelector(":scope > a")?.classList.add("parent");
    active = link;
    // On la garde visible dans la colonne, qui défile seule quand elle dépasse l'écran.
    const box = toc.getBoundingClientRect(), item = link.getBoundingClientRect();
    if (item.top < box.top + 30 || item.bottom > box.bottom - 30) {
      toc.scrollTop += item.top - box.top - box.height / 3;
    }
  }

  let queued = false;
  addEventListener("scroll", () => {
    if (!queued) {
      queued = true;
      requestAnimationFrame(() => { queued = false; update(); });
    }
  }, { passive: true });
  addEventListener("resize", update);
  update();
})();
