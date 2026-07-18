# Mjolnir Modular Builder — Windows EXE source

This repository builds a standalone Windows desktop configurator for the modular classic-comics Mjolnir system.

## Build the EXE using GitHub Actions

1. Create a new empty GitHub repository.
2. Upload **all contents of this folder**, including the hidden `.github` folder.
3. Commit the files to the `main` branch.
4. Open the repository's **Actions** tab.
5. Select **Build Windows EXE** and choose **Run workflow**.
6. When the workflow finishes, open the run and download the artifact named **Mjolnir-Modular-Builder-Windows**.
7. Extract it. The artifact contains:
   - an NSIS `Setup.exe` installer;
   - an MSI installer;
   - the directly built application `.exe`.

GitHub Actions builds on an actual Windows runner. No development tools need to be installed on your PC.

## What the program does

- previews assembled combinations in interactive 3D;
- selects real-leather cores or detailed printed-leather handles;
- selects 170, 195 or 220 mm lengths;
- selects three thicknesses per handle system;
- selects comic pommel or no pommel;
- selects no strap, real-leather preview, plain TPU or detailed TPU;
- changes metal, leather and strap colours;
- exports a pre-coloured Bambu-style 3MF;
- places every selected printable component on its own single-colour build plate.

## Included model libraries

The five source 3MF libraries are bundled in `assets/` and compiled into the Windows application package.

## Notes

Windows may show a SmartScreen warning because the executable is not code-signed. Choose **More info → Run anyway** only when the file came from your own GitHub Actions build.
