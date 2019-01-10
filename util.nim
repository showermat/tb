import strutils
import unicode
import osproc
import selectors
import posix, os # Required for selectors

var ncolor* = 8

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

type ColorSpec* = ref object
    c256*: int
    c8*: int
    bg*: bool

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

proc wcwidthInner(c: int32): cint {.header: "<wchar.h>", importc: "wcwidth".}

proc wcwidth*(c: Rune): cint = wcwidthInner(c.int32)

proc newColorSpec*(c256: int, c8: int, bg: bool): ColorSpec =
    return ColorSpec(c256: c256, c8: c8, bg: bg)

proc startColor*(color: ColorSpec):  string =
    let prefix = if color.bg: "4" else: "3"
    if ncolor < 256: return "\e[" & prefix & $color.c8 & "m"
    else: return "\e[" & prefix & "8;5;" & $color.c256 & "m"

const
    endFg* = "\e[39m"
    endBg* = "\e[49m"

proc colorPrint*(color: ColorSpec, s: string):  string =
    return startColor(color) & s & (if color.bg: endBg else: endFg)

proc clearscr*(f: File) = f.write("\e[H\e[2J")

proc scr_init*(f: File) =
    setlocale(LC_ALL, "")
    tcgetattr(f.getFileHandle.cint, addr ios_default)
    ios_raw = ios_default
    cfmakeraw(addr ios_raw)
    let (ret, err) = execCmdEx("tput colors") # FIXME This is really not the best solution and we should just be using ncurses.
    if err == 0: ncolor = ret.strip.parseInt

proc scr_save*(f: File) =
    f.write("\e[?1049h\e[?25l\e[?1000h\e[?1006h")
    f.clearscr()
    tcsetattr(f.getFileHandle.cint, TCSANOW, addr ios_raw)

proc scr_restore*(f: File) =
    f.write("\e[?1049l\e[?25h\e[?1000l\e[?1006l")
    tcsetattr(f.getFileHandle.cint, TCSANOW, addr ios_default)

proc readchar(f: File, timeout: float = 0.0): char =
    if timeout == 0.0: return getc(f)
    let sel = newSelector[int]()
    sel.registerHandle(f.getFileHandle.int, {Read}, 0)
    if sel.select((timeout * 1000).int).len > 0: return getc(f)
    else: return '\0' # TODO Better sentinel

proc scrsize*(f: File): tuple[h: int, w: int] =
    var w: winsize
    ioctl(f.getFileHandle.cint, TIOCGWINSZ, addr w)
    return (int(w.ws_row), int(w.ws_col))

proc debugmsg*(f: File, msg: string) =
    f.write("\e[H\e[41;37m" & msg & "\e[m  ")
    discard f.readchar()

proc ctlseq*(f: File): (char, seq[int]) =
    proc int0(s: string): int =
        if s == "": 0 else: s.parseInt
    if f.readchar(0.1) != '[': return ('\0', @[])
    var parts: seq[int] = @[]
    var buf = ""
    while true:
        let c = f.readchar()
        case c
        of ';':
            parts &= buf.int0
            buf = ""
        of '0'..'9':
            buf &= c
        of '<': discard # FIXME
        else:
            parts &= buf.int0
            return (c, parts)
