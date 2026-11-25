# Force Layout

The graph starts on a golden-angle spiral. The same titles and the same order always produce
the same first frame, which makes screenshots and tests stable.

Then each step:

- pairs of nodes repel with a force proportional to `1 / distance²`;
- links behave like springs, pulling toward a rest length;
- a weak gravity pulls everything toward the world's centre;
- velocity is damped, so the layout settles instead of ringing forever.

Dragging a node pins it, so the simulation settles around your hand instead of fighting it.

## Cards

Past about a hundred nodes the settled layout only keeps the small dot markers apart, while
the graph still draws full cards for a Local neighbourhood and for the selection's neighbours
in Global. Once the layout has stopped, those drawn cards are packed apart to the largest card's
footprint in one pass: only the drawn nodes move, they keep their velocities at zero so nothing
rings, and the same vault with the same selection always lands on the same positions. The pack
runs when the drawn set changes — entering Local, choosing another note — not while you zoom or
pan, and it is an adjustment rather than an animation, so Reduce motion applies it too.

## Why deterministic

A layout that jumps on every launch is hard to trust. A layout that starts in the same place
and relaxes from there lets you build muscle memory for where notes live.

Back to [[Knowledge Graph]], or try the focused [[Local Graph]].
