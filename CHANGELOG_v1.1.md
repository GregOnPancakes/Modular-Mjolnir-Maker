# Mjolnir Modular Builder 1.1

- Fixed the blank 3D preview by compiling every 3MF model library directly into the application executable. The installed app no longer depends on locating a separate runtime `assets` folder.
- Added three handle constructions:
  - Flat / smooth core
  - Simple printed-leather detail with reduced relief
  - Detailed printed-leather detail matching the accepted reference style
- Added three thicknesses for each construction.
  - Flat cores: 25, 28 and 31 mm
  - Simple and detailed printed leather: 31, 34 and 37 mm maximum diameter
- Retained all three handle lengths, pommel choices, strap choices, colour controls, interactive preview and multi-plate 3MF export.
- Cached the embedded mesh libraries after first load to make later combination changes faster.
