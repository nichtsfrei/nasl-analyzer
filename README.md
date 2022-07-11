# nasl-analyzer

Is a lsp for nasl based upon rust-analyzer.

It is in a very early state and currently only supports:
- GotoDefinition to global functions and assignments within a single file

Next steps:
- add configuration to define script paths to enable more broader lookups
- load nasl scripts and includes based on include, script_dependencies to not have to load each nasl script but only used
- make GotoDefinition work for local scopes (e.g. in blocks, parameter_lists, etc)
- continue for more functions than GotoDefinition

## Include into neovim

In this example I assume that you have 
- a lua based configuration 
- [nvim-lsp-config](https://github.com/neovim/nvim-lspconfig) setup
- nasl-analyzer is installed and findable within your `PATH`

```
local configs = require("lspconfig.configs")

local util = require("lspconfig.util")

local function create_config()
  return {
    default_config = {
      cmd = { "nasl-analyzer" },
      filetypes = { "nasl" },
      root_dir = util.root_pattern("plugin_feed_info.inc", ".git"),
      single_file_support = true,
      -- optional additional nvt dirs
      settings = { paths = {"/var/lib/openvas/plugins"}},
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
