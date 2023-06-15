# Alpacka

## Rust-powered Neovim package manager

## Features

- [x] Blazingly fast Installs. Uses libgit2 directly rather than the git CLI.
- [x] Runs in parallel. Uses rayon to run installs in parallel.
- [x] Lockfile interface. "packages.json" contain all packages to be installed, with frontends being able to generate them. (No frontends yet)
- [x] Cache old versions of lockfiles into a file. This allows for fast rollbacks, as we just look at the previous lockfile's output.
- [x] Extremely fast rollbacks. Usually < 1 second as no resolvers are run.
- [x] CLI to install and inspect packages.

TODO

- [ ] Local packages
- [ ] Frontends (Neovim frontend, CLI frontend, etc)
- [ ] Patches
- [ ] Uninstalling packages (Currently they are just deleted from the lockfile, but not from the filesystem)
- [ ] Luarocks support
- [ ] Lazy loading though Neovim frontend
- [ ] Installing/managing neovim versions through the CLI frontend
- [ ] Updating packages through the CLI frontend, incrementing the generation in the lockfile and installing e.g new commit of a branch
- [ ] Conventional commits; Detect breaking changes in installed packages and warn the user.
- [ ] [Nvim pack spec](https://github.com/nvim-lua/nvim-package-specification) support
