#compdef gjob gbatch gcancel gqueue

# Enhanced dynamic completions for gflow (zsh)
# Place this in your fpath before the basic completions or source it after

# Helper function to get job IDs
_gflow_get_job_ids() {
    local config_arg=""
    # Extract --config from command line
    for ((i=1; i<=${#words[@]}; i++)); do
        if [[ "${words[i]}" == "--config" ]]; then
            config_arg="--config ${words[i+1]}"
            break
        fi
    done

    local jobs
    jobs=(${(f)"$(gqueue $config_arg --format id 2>/dev/null | tail -n +2 | awk '{print $1}')"})
    _describe 'job-id' jobs
}

# Helper function to get job IDs by state
_gflow_get_job_ids_by_state() {
    local state=$1
    local config_arg=""
    for ((i=1; i<=${#words[@]}; i++)); do
        if [[ "${words[i]}" == "--config" ]]; then
            config_arg="--config ${words[i+1]}"
            break
        fi
    done

    local jobs
    jobs=(${(f)"$(gqueue $config_arg --states "$state" --format id 2>/dev/null | tail -n +2 | awk '{print $1}')"})
    _describe 'job-id' jobs
}

# Helper function to get conda environments
_gflow_conda_envs() {
    if (( $+commands[conda] )); then
        local envs
        envs=(${(f)"$(conda env list 2>/dev/null | grep -v "^#" | awk '{print $1}' | grep -v "^$")"})
        _describe 'conda-env' envs
    fi
}

# Enhanced gjob completion
_gjob_dynamic() {
    local context state state_descr line
    typeset -A opt_args

    local curcontext="$curcontext"

    _arguments -C \
        '--config[Path to the config file]:config file:_files' \
        '*-v[Increase logging verbosity]' \
        '*--verbose[Increase logging verbosity]' \
        '*-q[Decrease logging verbosity]' \
        '*--quiet[Decrease logging verbosity]' \
        '(-h --help)'{-h,--help}'[Print help]' \
        '(-V --version)'{-V,--version}'[Print version]' \
        '1: :->command' \
        '*:: :->args' && return 0

    case $state in
        command)
            local commands=(
                'attach:Attach to a job'\''s tmux session'
                'a:Attach to a job'\''s tmux session'
                'log:View a job'\''s log output'
                'l:View a job'\''s log output'
                'hold:Put a queued job on hold'
                'h:Put a queued job on hold'
                'release:Release a held job back to the queue'
                'r:Release a held job back to the queue'
                'show:Show detailed information about a job'
                's:Show detailed information about a job'
                'redo:Resubmit a job with the same or modified parameters'
                'completion:Generate shell completion scripts'
                'help:Print help'
            )
            _describe 'command' commands
            ;;
        args)
            local cmd=$words[1]
            case $cmd in
                attach|a)
                    _arguments \
                        '(-j --job)'{-j,--job}'[Job ID to attach to]:job-id:->running-jobs' \
                        '(-h --help)'{-h,--help}'[Print help]'
                    case $state in
                        running-jobs) _gflow_get_job_ids_by_state "Running" ;;
                    esac
                    ;;
                log|l)
                    _arguments \
                        '(-j --job)'{-j,--job}'[Job ID to view log]:job-id:->running-jobs' \
                        '(-h --help)'{-h,--help}'[Print help]'
                    case $state in
                        running-jobs) _gflow_get_job_ids_by_state "Running" ;;
                    esac
                    ;;
                hold|h)
                    _arguments \
                        '(-j --job)'{-j,--job}'[Job IDs to hold]:job-ids:->queued-jobs' \
                        '(-h --help)'{-h,--help}'[Print help]'
                    case $state in
                        queued-jobs) _gflow_get_job_ids_by_state "Queued" ;;
                    esac
                    ;;
                release|r)
                    _arguments \
                        '(-j --job)'{-j,--job}'[Job IDs to release]:job-ids:->held-jobs' \
                        '(-h --help)'{-h,--help}'[Print help]'
                    case $state in
                        held-jobs) _gflow_get_job_ids_by_state "Held" ;;
                    esac
                    ;;
                show|s)
                    _arguments \
                        '(-j --job)'{-j,--job}'[Job IDs to show]:job-ids:->all-jobs' \
                        '(-h --help)'{-h,--help}'[Print help]'
                    case $state in
                        all-jobs) _gflow_get_job_ids ;;
                    esac
                    ;;
                redo)
                    _arguments \
                        '1:job-id:->all-jobs-or-at' \
                        '(-g --gpus)'{-g,--gpus}'[Override GPUs]:gpus:' \
                        '(-p --priority)'{-p,--priority}'[Override priority]:priority:' \
                        '(-d --depends-on)'{-d,--depends-on}'[Override dependency]:dep:->deps' \
                        '(-t --time)'{-t,--time}'[Override time limit]:time:' \
                        '(-e --conda-env)'{-e,--conda-env}'[Override conda env]:env:->conda-envs' \
                        '--clear-deps[Clear dependency]' \
                        '(-h --help)'{-h,--help}'[Print help]'
                    case $state in
                        all-jobs-or-at)
                            _alternative \
                                'jobs:job-id:_gflow_get_job_ids' \
                                'special:special:(@ @~1 @~2 @~3)'
                            ;;
                        conda-envs) _gflow_conda_envs ;;
                        deps)
                            _alternative \
                                'jobs:job-id:_gflow_get_job_ids' \
                                'special:special:(@ @~1 @~2 @~3)'
                            ;;
                    esac
                    ;;
            esac
            ;;
    esac
}

# Enhanced gbatch completion
_gbatch_dynamic() {
    local context state state_descr line
    typeset -A opt_args

    _arguments -C \
        '--config[Path to the config file]:config file:_files' \
        '(-c --conda-env)'{-c,--conda-env}'[Conda environment]:env:->conda-envs' \
        '(-g --gpus)'{-g,--gpus}'[GPU count]:gpus:' \
        '--priority[Priority]:priority:' \
        '--depends-on[Job dependency]:dep:->deps' \
        '--array[Job array spec]:array:' \
        '(-t --time)'{-t,--time}'[Time limit]:time:' \
        '(-n --name)'{-n,--name}'[Run name]:name:' \
        '(-h --help)'{-h,--help}'[Print help]' \
        '(-V --version)'{-V,--version}'[Print version]' \
        '1: :->command-or-script' \
        '*:: :->args' && return 0

    case $state in
        conda-envs) _gflow_conda_envs ;;
        deps)
            _alternative \
                'jobs:job-id:_gflow_get_job_ids' \
                'special:special:(@ @~1 @~2 @~3)'
            ;;
        command-or-script)
            _alternative \
                'commands:command:_command_names' \
                'files:file:_files'
            ;;
        args) _files ;;
    esac
}

# Enhanced gcancel completion
_gcancel_dynamic() {
    _arguments \
        '--config[Path to the config file]:config file:_files' \
        '--dry-run[Dry run mode]' \
        '(-h --help)'{-h,--help}'[Print help]' \
        '(-V --version)'{-V,--version}'[Print version]' \
        '1:job-ids:_gflow_get_job_ids'
}

# Register the functions
compdef _gjob_dynamic gjob
compdef _gbatch_dynamic gbatch
compdef _gcancel_dynamic gcancel
