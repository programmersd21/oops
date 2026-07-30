__oops_preexec() { command oops __internal-notify --cmd "$1" --cwd "$PWD" >/dev/null 2>&1 }
autoload -Uz add-zsh-hook
add-zsh-hook preexec __oops_preexec
