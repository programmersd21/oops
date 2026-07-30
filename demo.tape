Output demo.gif

Set Shell "bash"
Set FontSize 16
Set FontFamily "JetBrains Mono, Fira Code, monospace"
Set Theme { "name": "Black", "black": "#000000", "red": "#ff4444", "green": "#44ff44", "yellow": "#ffff44", "blue": "#4444ff", "magenta": "#ff44ff", "cyan": "#44ffff", "white": "#ffffff", "brightBlack": "#555555", "brightRed": "#ff6666", "brightGreen": "#66ff66", "brightYellow": "#ffff66", "brightBlue": "#6666ff", "brightMagenta": "#ff66ff", "brightCyan": "#66ffff", "brightWhite": "#ffffff", "background": "#000000", "foreground": "#ffffff", "selection": "#333333", "cursor": "#ffffff" }
Set Width 1000
Set Height 640
Set Padding 24
Set BorderRadius 8
Set WindowBar Colorful
Set WindowBarSize 32
Set Framerate 30
Set TypingSpeed 25ms

Hide
Sleep 500ms
Type `mkdir -p ~/project`
Enter
Type `cd ~/project`
Enter
Type `echo 'config=dev' > .env`
Enter
Type `mkdir -p assets`
Enter
Type `echo 'site=default' > assets/site.conf`
Enter
Type `echo 'theme=dark' > assets/theme.conf`
Enter
Type `oops daemon &`
Enter
Type `sleep 1`
Enter
Type `clear`
Enter
Sleep 500ms
Show
Sleep 500ms

Type `eval "$(oops init bash)"`
Enter
Sleep 1.5s

Type `oops status`
Enter
Sleep 3s

Type `printf 'db=prod\nhost=db.internal\n' > .env`
Enter
Sleep 1.2s

Type `printf 'site=oops-demo\nlang=en\n' > assets/site.conf`
Enter
Sleep 1.2s

Type `ls -la assets/`
Enter
Sleep 1.2s

Type `rm -rf assets`
Enter
Sleep 1.5s

Type `ls assets/`
Enter
Sleep 800ms

Type `oops list`
Enter
Sleep 3.5s
Type `j`
Sleep 500ms
Type `q`
Sleep 600ms

Type `oops diff 1`
Enter
Sleep 2.5s
Type `q`
Sleep 500ms

Type `oops`
Enter
Sleep 1.5s

Type `ls assets/`
Enter
Sleep 1.2s

Type `oops pin 1`
Enter
Sleep 1s

Type `oops gc`
Enter
Sleep 1.5s

Type `oops status`
Enter
Sleep 2.5s

Type `clear`
Enter
Sleep 400ms
Type `echo oops - because everyone makes mistakes`
Enter
Sleep 5s
