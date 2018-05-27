# Originally from https://gist.github.com/def-/58c3374c23f120e31872https://gist.github.com/def-/58c3374c23f120e31872

let ncolor = 256 # FIXME Setting this properly will probably require switching entirely to real ncurses

type termios {.header: "<termios.h>", importc: "struct termios".} = object # Do we really need to duplicate the object structure here?  So much for portability.
    c_iflag: cuint
    c_oflag: cuint
    c_cflag: cuint
    c_lflag: cuint
    c_line: cchar
    c_cc: array[32, cchar]
    c_ispeed: uint
    c_ospeed: uint

type winsize {.header: "<termios.h>", importc: "struct winsize".} = object
    ws_row: cushort
    ws_col: cushort
    ws_xpixel: cushort
    ws_ypixel: cushort

type ColorSet* = object
    c256: int
    c8: int
    bg: bool

var TCSANOW {.header: "<termios.h>", importc: "TCSANOW".}: cint

var TIOCGWINSZ {.header: "<sys/ioctl.h>", importc: "TIOCGWINSZ".}: culong

var LC_ALL {.header: "<locale.h>", importc: "LC_ALL".}: cint

var ios_default, ios_raw: termios

proc tcgetattr(fd: cint, termios_p: ptr termios) {.header: "<termios.h>", importc: "tcgetattr".}

proc tcsetattr(fd: cint, optional_actions: cint, termios_p: ptr termios) {.header: "<termios.h>", importc: "tcsetattr".}

proc cfmakeraw(termios_p: ptr termios) {.header: "<termios.h>", importc: "cfmakeraw".}

proc ioctl(fd: cint, request: culong, arg: pointer) {.header: "<sys/ioctl.h>", importc: "ioctl".}

proc getc(stream: File): char {.header: "<stdio.h>", importc: "getc".}

proc setlocale(category: cint, locale: cstring) {.header: "<locale.h>", importc: "setlocale".}

proc wcwidth*(c: int32): cint {.header: "<wchar.h>", importc: "wcwidth".}

proc newColorSet*(c256: int, c8: int, bg: bool): ColorSet =
    return ColorSet(c256: c256, c8: c8, bg: bg)

proc startColor*(color: ColorSet):  string =
    let prefix = if color.bg: "4" else: "3"
    if ncolor < 256: return "\e[" & prefix & $color.c8 & "m"
    else: return "\e[" & prefix & "8;5;" & $color.c256 & "m"

const endFg* = "\e[39m"

const endBg* = "\e[49m"

proc color*(color: ColorSet, s: string):  string =
    return startColor(color) & s & (if color.bg: endBg else: endFg)

proc style*(style: int, s: string): string = return "\e[" & $style & "m" & s & "\e[0m"

proc clearscr*(f: File) = f.write("\e[H\e[2J")

proc scr_init*(f: File) =
    setlocale(LC_ALL, "")
    tcgetattr(f.getFileHandle.cint, addr ios_default)
    ios_raw = ios_default
    cfmakeraw(addr ios_raw)

proc scr_save*(f: File) =
    f.write("\e[?1049h\e[?25l")
    f.clearscr()
    tcsetattr(f.getFileHandle.cint, TCSANOW, addr ios_raw)

proc scr_restore*(f: File) =
    f.write("\e[?1049l\e[?25h")
    tcsetattr(f.getFileHandle.cint, TCSANOW, addr ios_default)

proc readchar(f: File): char = return getc(f)

proc scrsize*(f: File): tuple[h: int, w: int] =
    var w: winsize
    ioctl(f.getFileHandle.cint, TIOCGWINSZ, addr w)
    return (int(w.ws_row), int(w.ws_col))

proc debugmsg*(f: File, msg: string) =
    f.write("\e[H\e[2K\e[41;37m" & msg & "\e[m")
    discard f.readchar()
