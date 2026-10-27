/**
 * A concept page's code sidebar. Every cited block is already on the page,
 * hidden; this only picks which one shows and whether the panel is open.
 *
 *   click a citation   its block (⌘/ctrl-click still opens GitHub)
 *   [ and ]            the previous and next citation in the prose
 *   Esc                close (on a phone the panel is a bottom drawer)
 */
export const mountCites = (): void => {
  const root = document.querySelector<HTMLElement>("[data-cites-root]");
  const panel = root?.querySelector<HTMLElement>(".cite-panel");
  if (!root || !panel) return;
  const blocks = new Map([...panel.querySelectorAll<HTMLElement>("[data-cite-block]")].map((b) => [b.dataset["citeBlock"]!, b]));
  const links = [...root.querySelectorAll<HTMLAnchorElement>(".prose a.cite[data-cite]")].filter((a) => blocks.has(a.dataset["cite"]!));
  const count = panel.querySelector<HTMLElement>(".cite-count");
  const wide = matchMedia("(min-width: 1200px)");
  const isOpen = (): boolean => root.dataset["cites"] === "open" || (wide.matches && root.dataset["cites"] !== "closed");
  let current = 0;

  const show = (i: number, open: boolean): void => {
    const link = links[i];
    if (!link) return;
    current = i;
    for (const other of links) {
      if (other === link) other.setAttribute("aria-current", "true");
      else other.removeAttribute("aria-current");
    }
    for (const [key, block] of blocks) block.hidden = key !== link.dataset["cite"];
    if (count) count.textContent = `${i + 1} / ${links.length}`;
    panel.scrollTop = 0;
    if (open) root.dataset["cites"] = "open";
  };
  const close = (): void => {
    root.dataset["cites"] = "closed";
  };
  const step = (by: number): void => {
    show(Math.min(links.length - 1, Math.max(0, current + by)), true);
    links[current]?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  };

  links.forEach((link, i) =>
    link.addEventListener("click", (event) => {
      if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey || event.button !== 0) return;
      event.preventDefault();
      show(i, true);
    }),
  );
  panel.querySelector("[data-cite-close]")?.addEventListener("click", close);
  for (const button of panel.querySelectorAll<HTMLElement>("[data-cite-step]")) {
    button.addEventListener("click", () => step(Number(button.dataset["citeStep"])));
  }
  document.addEventListener("keydown", (event) => {
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    const target = event.target as HTMLElement | null;
    if (target?.closest("input, textarea, select, [contenteditable]")) return;
    if (event.key === "Escape" && isOpen()) close();
    else if (event.key === "[" || event.key === "]") {
      event.preventDefault();
      step(event.key === "]" ? 1 : -1);
    }
  });
  show(0, false);
};
