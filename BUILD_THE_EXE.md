# Build the Windows EXE — exact steps

1. Go to GitHub and create a **new empty repository** named `Mjolnir-Modular-Builder`.
2. Do not add a README, licence or `.gitignore` when creating it.
3. Extract `Mjolnir_Builder_Tauri_GitHub_Source.zip` on your computer.
4. Open the extracted `Mjolnir_Builder_Tauri_GitHub` folder.
5. On the GitHub repository page choose **Add file → Upload files**.
6. Drag **everything inside** the extracted folder onto the upload page. The `.github` folder must be included.
7. Commit the upload to `main`.
8. Open the repository's **Actions** tab.
9. Select **Build Windows EXE** on the left.
10. Choose **Run workflow → Run workflow**.
11. Wait for the green tick.
12. Open the completed run and download **Mjolnir-Modular-Builder-Windows** under Artifacts.
13. Extract the artifact and run the `Setup.exe` from the NSIS folder.

The first build commonly takes several minutes because GitHub installs Rust and compiles Tauri.

## Windows SmartScreen

The program is not commercially code-signed. Windows may show a SmartScreen warning. For an EXE built in your own repository, choose **More info → Run anyway**.
