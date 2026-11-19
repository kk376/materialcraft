# Parity with the reference workflow

The [catalog status](node-catalog-parity.md) covers 29 main-graph entries, 85 function
operations, four map kinds and 954 loaded package functions. The
[atomic corrections](atomic-parity-improvements.md) describe filter, channel-selection
and background fixes with local regression coverage. Full numerical parity remains
unestablished.

> The assessment below predates the 2026-11-05 primitive rewrite. Its old library counts and
> compatibility claims are historical. See [atomic-workflow.md](atomic-workflow.md) and
> [ROADMAP.md](../ROADMAP.md) for the current scope and remaining validation.

Current integration also provides workspace-scoped `material_export`, real Height displacement
from the assigned material, explicit normal-map convention, and linear bitmap/input previews.
The native SBS and primitive-library limitations in [atomic-workflow.md](atomic-workflow.md) remain.

Reusable function instances now preserve SBS package paths and appear in the Library. The converted
standard library contains all 171 supplied function bodies (2,334 nodes), with no empty placeholders.
Local structural and evaluation comparisons do not establish external runtime interoperability,
full numerical equivalence or redistribution rights for the retained source bodies.

Explorer now preserves native SBS folder hierarchies and provides package, folder and graph context
menus for creation, copying, pasting, renaming, removal and package file actions. Moving items within
a package and removing folder subtrees support undo; empty folders survive saves.

> **Last reviewed:** 2026-11-02 · **Last updated:** 2026-11-18 · **Change:** minor (UI workflow update: menus, Preferences, New graph and Home) · **Target:** the professional node-based material workflow (desktop app; the web app is assessed later)

**Overall: ≈ 69%** of a material artist's daily workflow works end to end (engine + UI +
automation + tests), weighted by how much of the day each area carries. **Remaining: ≈ 83 Opus
5.5 agent hours** (see [roadmap.md](roadmap.md)).

| Area | Weight | Done | What works | Main gaps |
|---|---:|---:|---|---|
| Graph editing | 15 | 75% | Pan/zoom canvas with grid; node cards with headers by category, live thumbnails and size/format labels; typed ports; bezier wires; drag to wire (from an output or an unconnected input), re-route and unplug; select wires (click, Shift/Ctrl+click), delete them (Delete, Backspace, wire menu) and swap their sources (X); reroute dots pulled out of a wire (drag), dropped on it (double-click) or inserted from its menu; drop a wire on empty canvas to add a wired node; box/Ctrl/Shift selection; move; frames (move with contents, resize), comments (inline edit), navigation pins; node search (Space, double-click); library drag and drop; align and distribute; copy/cut/paste/duplicate (with or without links); delete and relink; disable; select upstream/downstream; create graph from selection; snapshot undo with merged slider drags | Dot-node chains editing, wire tension preferences, compact node display mode, multi-graph tabs, minimap |
| Atomic nodes | 12 | 95% | Bitmap, Blend (16 modes, masks, alpha), Blur, Channels Shuffle, Curve, Directional Blur, Directional Warp, Distance (with color propagation), Emboss, FX-Map (grid, quadtree and scatter modes with per-instance randomness), Gradient (Dynamic), Gradient Map, Grayscale Conversion, HSL, Levels, Normal, Pixel Processor, Value Processor, Sharpen, SVG (path data, fill rules, stroke), Text (own stroke font), Transformation 2D, Uniform Color, Warp, inputs, outputs, dots, graph instances | FX-Map driven by per-iteration functions, SVG arcs and SVG files, font choice for Text |
| Node library | 20 | 45% | 227 nodes: noises (tileable 3D Perlin, simplex and Worley, cracks, stones, dirt, grunge maps and more), patterns (tile generator and sampler, bricks, hexagons, weave, shape splatter, paint strokes, scratches), filters (adjustments, blending, blurs, channels, effects, normal map tools, tiling, transforms), the flood fill family, whole-material and height blending, weathering (rust, moss, leaks, edge damage, edge wear, dust) | The rest of the reference library's ~400 graphs: more grunge and scan processing, mesh-based generators and bakers, spline tools, SDF functions |
| Function graphs | 10 | 70% | A compiled, 4-wide evaluator with ~55 ops (constants, $pos/$size, exposed variables, samplers, vectors, swizzles, math, logic, comparison, noise, random, if/else); pixel and value processors; a visual function editor; parameter-owned functions on base parameters, native values and instance inputs, with typed outputs, exposure and undo | Loops, function graphs as reusable package items |
| Graph parameters and instances | 8 | 70% | Exposed inputs (float, int, bool, enum, color) bound to parameters of any compatible type, shown in groups, edited with one shared editor on the graph and on its instances; presets (save, apply, delete); sub-graph instances with their own inputs and outputs; graph attributes; `--set` on the CLI | Visibility conditions, embedded vs. linked packages |
| 2D view | 5 | 90% | The reference layout: a toolbar above the image (background image, save, copy to the clipboard, output choice, UV overlay, Information and Histogram panels) and one below (channel menu with C/R/G/B/A keys, checkerboard, premultiplied alpha, sRGB or linear display, tiling across the whole view, physical size, exact fit, 1:1, stepped zoom with a zoom field, zoom lock); an Information panel with the pixel under the pointer and a color sampler (values at the image's bit depth and as floats, HSV, position); a Histogram panel with channel choice, bit depth, square-root scale, used range and a selectable range; size/format line | Color picker to parameters, floating the two panels, UV sets beyond the first |
| 3D view | 10 | 75% | The reference layout: menus (Scene, Materials, Lights, Camera, Environment, Display, Renderer), a tool strip (scene browser, Select, Camera and Light tools, environment background, shadows, a displacement pop-up, the display helpers) and a bar with the channels, the color space and the scene; the material, lights, camera, environment and renderer in the Properties panel. Thirteen built-in meshes and OBJ, glTF and GLB meshes; nine shaders (the OpenPBR Surface model with coat, fuzz, thin film, transmission and subsurface, metallic/roughness with coated and subsurface variants, specular/glossiness, Blinn, Lambert, unlit and mesh information) with every parameter editable, switchable and replaced by a graph output; height by tessellation and displacement or parallax occlusion; an ambient light and two point lights with shadows; 4 procedural environments with rotation, exposure and an on/off switch; focal length, perspective or orthographic projection, viewpoints, framing, clip planes, a studio backdrop, a background color or the environment behind the mesh, and a render resolution of its own; wireframe, backface culling, grid, axes with a scale, bounding box, light helpers, vertex tangent space, scene statistics, resolution and frame rate; tiling, UV scale and physical size; alpha transparency and an alpha channel view; save and copy the render, export the tessellated mesh as OBJ, state files and a saved default state | A GPU renderer and a path tracer, a ground plane that catches shadows, several materials on one mesh, loading shader definitions, anisotropy, dispersion and thin-walled transmission in the renderer |
| Files | 8 | 70% | `.mcraft` packages with embedded bitmaps; `.mcmat` portable materials (selected root graph, reachable helper graphs and source assets) that TextureCraft paints with and re-cooks through the CLI at its own resolution, values and mesh maps; preview images for input nodes; an Export window (From graph, Batch at a chosen output size, and OpenPBR MaterialX baking and procedural MCMAT publishing) writing the checked outputs (PNG, JPEG, TGA, BMP, WEBP, EXR, HDR; DDS listed for later; 8/16/32 bits; name pattern; sRGB, linear or raw per output); bitmap import (PNG, JPEG, TGA, TIFF, BMP, WEBP, EXR, HDR); mesh import (OBJ, glTF, GLB); open, save, import and export in the browser; atomic writes; autosave; recent files | Package dependencies, SVG and font files as resources |
| UI and UX | 8 | 70% | Dockable/floatable panels (explorer, library, graph, 3D, 2D, properties, console, history), menus, main toolbar, graph toolbar with atomic quick-insert, library with live previews, properties with curve/gradient/color editors and a Levels editor over the input histogram, the reference File, Tools, Windows and Help menus (Save all, Cancel rendering, Suspend engine, main toolbar toggle, help and community links), New graph dialog with template cards, a category filter, width and height and a target package, export dialog, a paged Preferences window (General, Projects, Graph, 3D View, Library and Shortcuts pages with OK, Cancel and Apply; recent list length, invert zoom, numbered autosave copies, node search on Tab and on wire drop, outputs in the 3D view on open, node creation shortcuts), a Plugin Manager window (no plugin loading yet), shortcuts list, a home screen at startup (templates, recent packages, learn and community links), visual function editor, presets bar, accessible names on every widget, status bar | A plugin system, bakers, keymap editor for application commands, minimap, multi-graph tabs |
| Platforms | 4 | 50% | Windows, macOS, Linux, FreeBSD and web builds compile; browser file I/O; packaging scripts and release workflows | First release run |
| **Total** | **100** | **≈ 69%** | | |

Beyond the reference app: every edit is a command available through MCP, the CLI and the app's
control channel (including synthetic mouse and keyboard input), the engine runs headless with the
same results, and everything compiles to WebAssembly.

## Method

Scores are self-assessed from our code against public documentation of the professional
node-based material workflow and observation of the reference desktop app's menus, panels and
node library, area by area. A feature counts only when it works end to end (engine, UI, automation)
and is tested.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-18 | minor | Updated File, Tools, Windows and Help menus, paged Preferences, node creation shortcuts, New graph with independent sizes and a target package, a Plugin Manager window and Home (#269). The historical scores remain as recorded |
| 2026-11-15 | major | 3D view 55% → 75%: the reference menus, tool strip and properties; shaders with editable parameters, lights with shadows, camera lens and projection, render resolution, display helpers, state files (#221); overall ≈ 67% → ≈ 69% |
| 2026-11-14 | minor | 2D view 80% → 90%: the reference toolbars, the Information and Histogram panels, display settings and stepped zoom (#221); overall unchanged at ≈ 67% |
| 2026-11-02 | minor | ≈ 66% → ≈ 67%: `.mcmat` portable materials and TextureCraft interop (Files 70%) |
| 2026-11-02 | major | Function editor, dynamic parameters, presets, 227 nodes (FX-Map, SVG, Text, 3D noises, weathering), mesh import, browser file I/O, histogram: ≈ 51% → ≈ 66% |
| 2026-11-02 | major | First assessment after the first day of work |
