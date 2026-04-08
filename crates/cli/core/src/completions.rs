use crate::metadata::{CliDumpKind, CliQuery, CliShell, CliTopic, wm_command_specs};

pub fn render_completion_script(shell: CliShell) -> String {
    match shell {
        CliShell::Zsh => render_zsh_completion(),
        CliShell::Bash => render_bash_completion(),
        CliShell::Fish => render_fish_completion(),
    }
}

fn render_zsh_completion() -> String {
    let queries = join_words(CliQuery::ALL.into_iter().map(CliQuery::name));
    let commands = join_words(wm_command_specs().iter().map(|spec| spec.name));
    let topics = join_words(CliTopic::ALL.into_iter().map(CliTopic::name));
    let dumps = join_words(CliDumpKind::ALL.into_iter().map(CliDumpKind::name));

    format!(
        r#"#compdef spiders-cli

local -a _spiders_cli_queries
local -a _spiders_cli_commands
local -a _spiders_cli_topics
local -a _spiders_cli_dumps

_spiders_cli_queries=({queries})
_spiders_cli_commands=({commands})
_spiders_cli_topics=({topics})
_spiders_cli_dumps=({dumps})

_arguments -C \
  '--json[emit JSON output]' \
  '--socket[path to WM IPC socket]:socket path:_files' \
  '1:command:(config wm completions)' \
  '2:subcommand:->subcommand' \
  '3:arg:->arg' \
  '4:arg:->arg' \
  '5:arg:->arg'

case $state in
  subcommand)
    case $words[2] in
      config)
        _values 'config command' discover check build
        ;;
      wm)
        _values 'wm command' query command monitor debug smoke
        ;;
      completions)
        _values 'shell' zsh bash fish
        ;;
    esac
    ;;
  arg)
    case "$words[2] $words[3] $words[4]" in
      'wm query '*)
        _describe 'query' _spiders_cli_queries
        ;;
      'wm command '*)
        _describe 'command' _spiders_cli_commands
        ;;
      'wm monitor '*)
        _describe 'topic' _spiders_cli_topics
        ;;
      'wm debug dump'*)
        _describe 'dump kind' _spiders_cli_dumps
        ;;
      'completions '*)
        _values 'shell' zsh bash fish
        ;;
    esac
    ;;
esac
"#
    )
}

fn render_bash_completion() -> String {
    let queries = join_words(CliQuery::ALL.into_iter().map(CliQuery::name));
    let commands = join_words(wm_command_specs().iter().map(|spec| spec.name));
    let topics = join_words(CliTopic::ALL.into_iter().map(CliTopic::name));
    let dumps = join_words(CliDumpKind::ALL.into_iter().map(CliDumpKind::name));

    format!(
        r#"_spiders_cli() {{
  local cur prev words cword
  _init_completion || return

  case "${{COMP_WORDS[1]}}" in
    config)
      COMPREPLY=( $(compgen -W "discover check build" -- "$cur") )
      ;;
    wm)
      case "${{COMP_WORDS[2]}}" in
        query)
          COMPREPLY=( $(compgen -W "{queries}" -- "$cur") )
          ;;
        command)
          COMPREPLY=( $(compgen -W "{commands}" -- "$cur") )
          ;;
        monitor)
          COMPREPLY=( $(compgen -W "{topics}" -- "$cur") )
          ;;
        debug)
          if [[ "${{COMP_WORDS[3]}}" == "dump" ]]; then
            COMPREPLY=( $(compgen -W "{dumps}" -- "$cur") )
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
}}

complete -F _spiders_cli spiders-cli
"#
    )
}

fn render_fish_completion() -> String {
    let query_lines = CliQuery::ALL
        .into_iter()
        .map(|query| {
            format!(
                "complete -c spiders-cli -n '__fish_seen_subcommand_from wm; and __fish_seen_subcommand_from query' -a '{}' -d '{}'",
                query.name(),
                query.help()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let command_lines = wm_command_specs()
        .iter()
        .map(|spec| {
            format!(
                "complete -c spiders-cli -n '__fish_seen_subcommand_from wm; and __fish_seen_subcommand_from command' -a '{}' -d '{}'",
                spec.name,
                spec.help
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let topic_lines = CliTopic::ALL
        .into_iter()
        .map(|topic| {
            format!(
                "complete -c spiders-cli -n '__fish_seen_subcommand_from wm; and __fish_seen_subcommand_from monitor' -a '{}' -d '{}'",
                topic.name(),
                topic.help()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let dump_lines = CliDumpKind::ALL
        .into_iter()
        .map(|kind| {
            format!(
                "complete -c spiders-cli -n '__fish_seen_subcommand_from wm; and __fish_seen_subcommand_from dump' -a '{}' -d '{}'",
                kind.name(),
                kind.help()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"complete -c spiders-cli -l json -d 'emit JSON output'
complete -c spiders-cli -l socket -r -d 'path to WM IPC socket'
complete -c spiders-cli -f -n 'not __fish_seen_subcommand_from config wm completions' -a 'config wm completions'
complete -c spiders-cli -n '__fish_seen_subcommand_from config' -a 'discover check build'
complete -c spiders-cli -n '__fish_seen_subcommand_from wm; and not __fish_seen_subcommand_from query command monitor debug smoke' -a 'query command monitor debug smoke'
complete -c spiders-cli -n '__fish_seen_subcommand_from completions' -a 'zsh bash fish'
complete -c spiders-cli -n '__fish_seen_subcommand_from wm; and __fish_seen_subcommand_from debug; and not __fish_seen_subcommand_from dump' -a 'dump'
{query_lines}
{command_lines}
{topic_lines}
{dump_lines}
"#
    )
}

fn join_words(words: impl IntoIterator<Item = &'static str>) -> String {
    words.into_iter().collect::<Vec<_>>().join(" ")
}
