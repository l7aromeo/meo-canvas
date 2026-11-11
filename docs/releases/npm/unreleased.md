### Fixed

- A line whose leading is odd or not a whole pixel puts its baseline where Chrome does, half the leading floored, for a box taller or shorter than its text alike: 12px Oswald at `lineHeight: 1` drew its baseline 11.5px below the line's top where Chrome draws it at 11, and 13px at `lineHeight: 1.5` at 15.75 where Chrome draws it at 15. Text on such lines drew up to three quarters of a pixel low, and `alignItems: 'baseline'` aligned to that baseline. Boxes are unchanged (`line-height-normal.tsv`, `l7aromeo/meo-canvas#233`).
