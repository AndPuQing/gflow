# Enhanced dynamic completions for gflow (fish)
# Place in ~/.config/fish/completions/ or /usr/share/fish/completions/

# Helper function to get job IDs
function __gflow_get_job_ids
    set -l config_arg ""
    # Extract --config from command line
    for i in (seq (count $argv))
        if test "$argv[$i]" = "--config"
            set config_arg "--config $argv[(math $i + 1)]"
            break
        end
    end

    gqueue $config_arg --format id 2>/dev/null | tail -n +2 | awk '{print $1}'
end

# Helper function to get job IDs by state
function __gflow_get_job_ids_by_state
    set -l state $argv[1]
    set -l config_arg ""

    gqueue $config_arg --states "$state" --format id 2>/dev/null | tail -n +2 | awk '{print $1}'
end

# Helper function to get conda environments
function __gflow_get_conda_envs
    if command -v conda >/dev/null 2>&1
        conda env list 2>/dev/null | grep -v '^#' | awk '{print $1}' | grep -v '^$'
    end
end

# gjob attach - complete with running jobs
complete -c gjob -n "__fish_seen_subcommand_from attach a" -s j -l job -d "Job ID to attach to" -a "(__gflow_get_job_ids_by_state Running)"

# gjob log - complete with running jobs
complete -c gjob -n "__fish_seen_subcommand_from log l" -s j -l job -d "Job ID to view log" -a "(__gflow_get_job_ids_by_state Running)"

# gjob hold - complete with queued jobs
complete -c gjob -n "__fish_seen_subcommand_from hold h" -s j -l job -d "Job IDs to hold" -a "(__gflow_get_job_ids_by_state Queued)"

# gjob release - complete with held jobs
complete -c gjob -n "__fish_seen_subcommand_from release r" -s j -l job -d "Job IDs to release" -a "(__gflow_get_job_ids_by_state Held)"

# gjob show - complete with all jobs
complete -c gjob -n "__fish_seen_subcommand_from show s" -s j -l job -d "Job IDs to show" -a "(__gflow_get_job_ids)"

# gjob redo - complete with all jobs and special @ syntax
complete -c gjob -n "__fish_seen_subcommand_from redo" -d "Job ID" -a "(__gflow_get_job_ids)"
complete -c gjob -n "__fish_seen_subcommand_from redo" -d "Most recent job" -a "@"
complete -c gjob -n "__fish_seen_subcommand_from redo" -d "2nd most recent" -a "@~1"
complete -c gjob -n "__fish_seen_subcommand_from redo" -d "3rd most recent" -a "@~2"

# gjob redo - conda environment completion
complete -c gjob -n "__fish_seen_subcommand_from redo" -s e -l conda-env -d "Override conda env" -a "(__gflow_get_conda_envs)"

# gjob redo - dependency completion
complete -c gjob -n "__fish_seen_subcommand_from redo" -s d -l depends-on -d "Override dependency" -a "(__gflow_get_job_ids)"
complete -c gjob -n "__fish_seen_subcommand_from redo" -s d -l depends-on -d "Most recent job" -a "@"

# gbatch - conda environment completion
complete -c gbatch -s c -l conda-env -d "Conda environment" -a "(__gflow_get_conda_envs)"

# gbatch - dependency completion
complete -c gbatch -l depends-on -d "Job dependency" -a "(__gflow_get_job_ids)"
complete -c gbatch -l depends-on -d "Most recent job" -a "@"
complete -c gbatch -l depends-on -d "2nd most recent" -a "@~1"
complete -c gbatch -l depends-on -d "3rd most recent" -a "@~2"

# gcancel - complete with job IDs
complete -c gcancel -n "not __fish_seen_subcommand_from completion help" -d "Job ID" -a "(__gflow_get_job_ids)"

# gqueue - state completion
complete -c gqueue -s s -l states -d "Filter by states" -a "Queued Running Held Completed Failed Cancelled"

# gqueue - format field completion
complete -c gqueue -s f -l format -d "Fields to display" -a "id state time name gpus priority command depends_on"
