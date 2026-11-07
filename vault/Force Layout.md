# Force Layout

The graph starts on a golden-angle spiral. The same titles and the same order always produce
the same first frame, which makes screenshots and tests stable.

Then each step:

- pairs of nodes repel with a force proportional to `1 / distance²`;
- links behave like springs, pulling toward a rest length;
- a weak gravity pulls everything toward the world's centre;
- velocity is damped, so the layout settles instead of ringing forever.

Dragging a node pins it, so the simulation settles around your hand instead of fighting it.

## Why deterministic

A layout that jumps on every launch is hard to trust. A layout that starts in the same place
and relaxes from there lets you build muscle memory for where notes live.

Back to [[Knowledge Graph]], or try the focused [[Local Graph]].
