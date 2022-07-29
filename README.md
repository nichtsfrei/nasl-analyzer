# nasl-analyzer

Is a lsp for nasl based upon rust-analyzer.

It is in a very early state and currently only supports:
- GotoDefinition 

Next steps:
- add References handling
- minimize memory footprint by not caching all plugins at once

## How to install

```
cargo install --path .
```

Installs `nasl-analyzer` into `$HOME/.cargo/bin`

## Include into neovim

In this example I assume that you have 
- a lua based configuration 
- [nvim-lsp-config](https://github.com/neovim/nvim-lspconfig) setup
- nasl-analyzer is installed and findable within your `PATH`

```
local configs = require("lspconfig.configs")
local util = require("lspconfig.util")
local os = require("os")

local function create_config()
  return {
    default_config = {
      cmd = { "nasl-analyzer" },
      filetypes = { "nasl" },
      root_dir = util.root_pattern("example.nasl", ".git"),
      single_file_support = true,
      settings = {
          paths = {},
          openvas = os.getenv("HOME") .. "/src/greenbone/openvas-scanner",
      },
    },
    docs = {
      description = [[
]]     ,
    },
  }
end

configs.nasl = create_config()
```

Afterwards you can register nasl lsp as usual.
