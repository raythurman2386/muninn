# Force Layout

The graph starts on a golden-angle spiral. The same titles and the same order always produce
the same first frame, which makes screenshots and tests stable.

Then each step:

- pairs of nodes repel with a force proportional to `1 / distance²`;
- links behave like springs, pulling toward a rest length;
- a weak gravity pulls everything toward the world's centre;
- velocity is damped, so the layout settles instead of ringing forever.

How long it settles is not a fixed timer. Each step the layout measures how far its nodes
moved on average: while they are still visibly rearranging it cools slowly, and once the
motion is too small to see it cools at the faster original rate. A large vault therefore
gets about four seconds at 60 frames a second to arrange itself (a frame budget, not a
clock, so a faster display reaches the same positions sooner), a small one still finishes
in about the time it did before, and a cap always stops it.

Dense vaults also get a soft edge: as the blob grows, nodes near the world's rim are eased
back toward the centre, so a long settle uses the room it has instead of piling up on the
hard wall. Small vaults never take that branch.

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
and relaxes from there lets you build muscle memory for where notes live. The settling itself
is a fixed sequence of steps — the progress rule only chooses the step size of the cooling
timer — so the same vault with the same order always lands on the same positions, not just the
same first frame.

Back to [[Knowledge Graph]], or try the focused [[Local Graph]].
