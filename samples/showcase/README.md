# Showcase images

Real output from chutao-djs, a Discord bot that renders its cards through
meo-canvas. They are here because the README links them, and an image host
outside the project is not somewhere a repository should depend on — the
previous copies lived on i.ibb.co, where nothing guaranteed they would still
be there.

**Nothing regenerates these.** The scripts in `scripts/` each write one fixed
filename into `samples/` — `chart_samples.png`, `sample_grids.png`,
`sample_nested_grids.png` and `sample_animated_card.webp` — and never glob or
clean, so this directory is out of their reach by construction rather than by
convention. Replacing an image here means exporting a new one from the bot and
committing it.

**They were exported before 8.0.0.** That release changed where text sits — line boxes now come
from the face's own metrics — so these show the older spacing. Re-exporting them from the bot is
what brings them back in step; nothing in this repository can do it.

The generated samples live one level up and are rebuilt with:

```bash
bun run generate:samples
```
