# File icons

The SVGs in this folder are from [Catppuccin VS Code Icons](https://github.com/catppuccin/vscode-icons),
**Mocha** flavor (`icons/mocha/`), at commit `b6915da9f6889b683a110aa747de96c2820a537d`.
They're MIT-licensed; see `LICENSE` here, which must stay next to them.

Only the icons the app maps to are copied (see `src/app/file_icon.rs`). To add one, copy
it from the same folder of that repo and add it to the mapping. To update, re-download
the files from a newer commit and change the hash above.
