_spiders_cli() {
  local cur prev words cword
  _init_completion || return

  case "${COMP_WORDS[1]}" in
    config)
      COMPREPLY=( $(compgen -W "discover check build" -- "$cur") )
      ;;
    wm)
      case "${COMP_WORDS[2]}" in
        query)
          COMPREPLY=( $(compgen -W "state focused-window current-output current-workspace monitor-list workspace-names" -- "$cur") )
          ;;
        command)
          COMPREPLY=( $(compgen -W "close-focused-window toggle-floating toggle-fullscreen reload-config focus-next-window focus-previous-window select-next-workspace select-previous-workspace cycle-layout-next cycle-layout-previous focus-left focus-right focus-up focus-down set-layout:<name> select-workspace:<id> spawn:<command>" -- "$cur") )
          ;;
        monitor)
          COMPREPLY=( $(compgen -W "all focus windows workspaces layout config" -- "$cur") )
          ;;
        debug)
          if [[ "${COMP_WORDS[3]}" == "dump" ]]; then
            COMPREPLY=( $(compgen -W "wm-state debug-profile scene-snapshot frame-sync seats" -- "$cur") )
          else
            COMPREPLY=( $(compgen -W "dump" -- "$cur") )
          fi
          ;;
        *)
          COMPREPLY=( $(compgen -W "query command monitor debug smoke" -- "$cur") )
          ;;
      esac
      ;;
    completions)
      COMPREPLY=( $(compgen -W "zsh bash fish" -- "$cur") )
      ;;
    --socket)
      compopt -o filenames
      ;;
    *)
      COMPREPLY=( $(compgen -W "--json --socket config wm completions" -- "$cur") )
      ;;
  esac
}

complete -F _spiders_cli spiders-cli
