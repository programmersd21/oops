__oops_preexec() {
    local cmd="$1"
    # Snapshot must complete before Bash executes the command. This is deliberately
    # synchronous — the daemon acks only after the snapshot is durable.
    command oops __internal-notify --cmd "$cmd" --cwd "$PWD" >/dev/null 2>&1
}
trap '__oops_preexec "$BASH_COMMAND"' DEBUG
