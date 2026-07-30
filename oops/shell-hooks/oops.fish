function __oops_preexec --on-event fish_preexec
    command oops __internal-notify --cmd "$argv" --cwd "$PWD" >/dev/null 2>&1
end
