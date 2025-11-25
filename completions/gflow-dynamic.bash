#!/usr/bin/env bash
# Enhanced dynamic completions for gflow
# Source this file after the basic completions are loaded

# Helper function to get job IDs
_gflow_get_job_ids() {
    local config_arg=""
    # Extract --config argument if present
    for ((i=1; i<COMP_CWORD; i++)); do
        if [[ "${COMP_WORDS[i]}" == "--config" ]]; then
            config_arg="--config ${COMP_WORDS[i+1]}"
            break
        fi
    done

    # Get job IDs from gqueue (only IDs of non-completed jobs by default)
    gqueue $config_arg --format id 2>/dev/null | tail -n +2 | awk '{print $1}'
}

# Helper function to get job IDs for specific states
_gflow_get_job_ids_by_state() {
    local state="$1"
    local config_arg=""
    for ((i=1; i<COMP_CWORD; i++)); do
        if [[ "${COMP_WORDS[i]}" == "--config" ]]; then
            config_arg="--config ${COMP_WORDS[i+1]}"
            break
        fi
    done

    gqueue $config_arg --states "$state" --format id 2>/dev/null | tail -n +2 | awk '{print $1}'
}

# Helper function to get conda environments
_gflow_get_conda_envs() {
    if command -v conda &> /dev/null; then
        conda env list 2>/dev/null | grep -v "^#" | awk '{print $1}' | grep -v "^$"
    fi
}

# Enhanced completion for gjob
_gjob_dynamic() {
    local cur prev words cword
    _init_completion || return

    local subcommand=""
    for ((i=1; i<cword; i++)); do
        case "${words[i]}" in
            attach|a|log|l|hold|h|release|r|show|s|redo)
                subcommand="${words[i]}"
                break
                ;;
        esac
    done

    case "$prev" in
        -j|--job)
            case "$subcommand" in
                attach|a|log|l|redo)
                    # For attach/log, suggest running jobs
                    COMPREPLY=( $(compgen -W "$(_gflow_get_job_ids_by_state Running)" -- "$cur") )
                    ;;
                hold|h)
                    # For hold, suggest queued jobs
                    COMPREPLY=( $(compgen -W "$(_gflow_get_job_ids_by_state Queued)" -- "$cur") )
                    ;;
                release|r)
                    # For release, suggest held jobs
                    COMPREPLY=( $(compgen -W "$(_gflow_get_job_ids_by_state Held)" -- "$cur") )
                    ;;
                show|s)
                    # For show, suggest all jobs
                    COMPREPLY=( $(compgen -W "$(_gflow_get_job_ids)" -- "$cur") )
                    ;;
            esac
            return 0
            ;;
        -c|--conda-env|-e)
            # Complete conda environments
            COMPREPLY=( $(compgen -W "$(_gflow_get_conda_envs)" -- "$cur") )
            return 0
            ;;
        -d|--depends-on)
            # Complete with job IDs or special @ syntax
            COMPREPLY=( $(compgen -W "$(_gflow_get_job_ids) @ @~1 @~2 @~3" -- "$cur") )
            return 0
            ;;
    esac

    # If first argument after subcommand and no flag, might be job ID
    if [[ $cword -eq 2 && -n "$subcommand" && "$cur" != -* ]]; then
        case "$subcommand" in
            attach|a|log|l)
                COMPREPLY=( $(compgen -W "$(_gflow_get_job_ids_by_state Running)" -- "$cur") )
                return 0
                ;;
            redo)
                COMPREPLY=( $(compgen -W "$(_gflow_get_job_ids) @" -- "$cur") )
                return 0
                ;;
        esac
    fi
}

# Enhanced completion for gbatch
_gbatch_dynamic() {
    local cur prev words cword
    _init_completion || return

    case "$prev" in
        -c|--conda-env)
            COMPREPLY=( $(compgen -W "$(_gflow_get_conda_envs)" -- "$cur") )
            return 0
            ;;
        --depends-on)
            COMPREPLY=( $(compgen -W "$(_gflow_get_job_ids) @ @~1 @~2 @~3" -- "$cur") )
            return 0
            ;;
    esac
}

# Enhanced completion for gcancel
_gcancel_dynamic() {
    local cur prev words cword
    _init_completion || return

    # If no flags yet, suggest job IDs
    if [[ "$cur" != -* && $cword -eq 1 ]]; then
        COMPREPLY=( $(compgen -W "$(_gflow_get_job_ids)" -- "$cur") )
        return 0
    fi
}

# Hook into existing completion functions
if declare -F _gjob > /dev/null; then
    complete -F _gjob_dynamic gjob
fi

if declare -F _gbatch > /dev/null; then
    complete -F _gbatch_dynamic gbatch
fi

if declare -F _gcancel > /dev/null; then
    complete -F _gcancel_dynamic gcancel
fi
