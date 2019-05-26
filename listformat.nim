import unicode
import tables
import strutils
import util

const tabwidth = 4

type
    FmtCmdType = enum
        fmtContain,
        fmtColor,
        fmtNobreak,
        fmtLiteral,
        fmtExclude
    FmtCmd* = ref object
        case kind: FmtCmdType
        of fmtContain:
            children: seq[FmtCmd]
        of fmtColor:
            colorset: ColorSpec
            colorContent: FmtCmd
        of fmtNobreak:
            nobreakContent: FmtCmd
        of fmtLiteral:
            literalContent: string
        of fmtExclude:
            excludeContent: FmtCmd

proc fcat*(children: seq[FmtCmd]): FmtCmd =
    return FmtCmd(kind: fmtContain, children: children)

proc fcolor*(color: ColorSpec, content: FmtCmd): FmtCmd =
    return FmtCmd(kind: fmtColor, colorset: color, colorContent: content)

proc fnobreak*(content: FmtCmd): FmtCmd =
    return FmtCmd(kind: fmtNobreak, nobreakContent: content)

proc flit*(s: string): FmtCmd =
    return FmtCmd(kind: fmtLiteral, literalContent: s)

proc fexclude*(content: FmtCmd = FmtCmd(kind: fmtLiteral, literalContent: "")): FmtCmd =
    return FmtCmd(kind: fmtExclude, excludeContent: content)

type StyleSet = ref object
    fg: ColorSpec
    bg: ColorSpec

proc override(style: StyleSet, color: ColorSpec): StyleSet =
    if color.bg: return StyleSet(fg: style.fg, bg: color)
    else: return StyleSet(fg: color, bg: style.bg)

proc start(style: StyleSet): string =
    var ret = ""
    if style.fg != nil: ret &= startColor(style.fg)
    if style.bg != nil: ret &= startColor(style.bg)
    return ret

proc stop(style: StyleSet): string =
    var ret = ""
    if style.fg != nil: ret &= endFg
    if style.bg != nil: ret &= endBg
    return ret

type Preformatted* = ref object
    width: int
    value: seq[string]
    raw: seq[string]
    mapping: Table[(int, int), (int, int)]

proc initPreformatted(width: int): Preformatted =
    return Preformatted(width: width, value: @[], raw: @[""], mapping: initTable[(int, int), (int, int)]())

proc append(target: var seq[string], value: seq[string]) =
    if value.len == 0: return
    if target.len == 0: target &= value
    else:
        target[^1] &= value[0]
        target &= value[1..^1]

proc internalFormat(output: Preformatted, value: FmtCmd, startcol: int, style: StyleSet, record: bool): int =
    case value.kind
    of fmtContain:
        var curcol = startcol
        for child in value.children:
            curcol = internalFormat(output, child, curcol, style, record)
        return curcol
    of fmtColor:
        return internalFormat(output, value.colorContent, startcol, style.override(value.colorset), record)
    of fmtNobreak:
        var sub = initPreformatted(0)
        let childlen = internalFormat(sub, value.nobreakContent, 0, style, record)
        if sub.value.len == 0: return startcol
        assert sub.value.len == 1 # TODO Support hard wraps in nobreak
        let rawstart = (output.raw.len - 1, output.raw[^1].len)
        let valstart =
            if output.value.len > 0: (output.value.len - 1, output.value[^1].len)
            else: (0, 0)
        for k, v in sub.mapping.pairs:
            output.mapping[(k[0] + rawstart[0], if k[0] == 0: k[1] + rawstart[1] else: k[1])] = (v[0] + valstart[0], if v[0] == 0: v[1] + valstart[1] else: valstart[1])
        output.raw.append(sub.raw)
        if output.width == 0 or childlen <= output.width - startcol:
            output.value.append(sub.value)
            return startcol + childlen
        assert childlen <= output.width # TODO Support this
        output.value &= sub.value
        return childlen
    of fmtLiteral:
        var cur = style.start
        var cnt = startcol
        var needMapping = true
        proc newline() =
            output.value.append(@[cur & style.stop, ""])
            cur = style.start
            cnt = 0
            needMapping = true
        for c in value.literalContent.runes:
            case c.int32
            of 10: # \n
                newline()
            of 9: # \t
                if output.width > 0 and cnt >= output.width - 4: newline()
                cur &= "    " # TODO What if width < 4?
                cnt += tabwidth
                needMapping = true
            else:
                let cw = max(c.wcwidth, 0)
                if output.width > 0 and cnt + cw > output.width: newline()
                cur &= c.toUTF8
                cnt += cw
            if record:
                output.raw[^1] &= c.toUTF8
                if needMapping and cnt > 0:
                    proc addMapping(charlen: int, offset: int) =
                        let target =
                            if output.value.len > 0: (output.value.len - 1, output.value[^1].runeLen + cur.runeLen - charlen)
                            else: (0, cur.runeLen - charlen)
                        output.mapping[(output.raw.len - 1, output.raw[^1].runeLen - offset)] = target
                    if c.int32 == 9:
                        addMapping(tabwidth, 1)
                        addMapping(0, 0) # Only necessary for tabs at end of line
                    else:
                        addMapping(1, 1)
                    needMapping = false
        output.value.append(@[cur & style.stop])
        return cnt
    of fmtExclude:
        output.raw &= ""
        return internalFormat(output, value.excludeContent, startcol, style, false)

proc format*(value: FmtCmd, width: int): Preformatted =
    var ret = initPreformatted(width)
    assert width > 0 # Ensured by caller in ListNode
    discard internalFormat(ret, value, 0, StyleSet(fg: nil, bg: nil), true)
    return ret

proc contains*(value: FmtCmd, q: string): bool = # Search a value without having to preformat it
    case value.kind
    of fmtContain:
        for child in value.children:
            if child.contains(q): return true
        return false
    of fmtColor:
        return value.colorContent.contains(q)
    of fmtNobreak:
        return value.nobreakContent.contains(q)
    of fmtLiteral:
        return value.literalContent.contains(q)
    of fmtExclude:
        return false

proc len*(p: Preformatted): int =
    return p.value.len

proc `[]`*(p: Preformatted, i: int): string =
    return p.value[i]

proc translate*(p: Preformatted, chunk: int, idx: int): (int, int) =
    var prev = -1
    for k in p.mapping.keys: # TODO We should be using a data structure that doesn't require O(n) search to translate
        if k[0] == chunk and k[1] > prev and k[1] <= idx: prev = k[1]
    assert prev != -1
    let delta = idx - prev
    let target = p.mapping[(chunk, prev)]
    return (target[0], target[1] + delta)

proc search*(p: Preformatted, q: string): seq[((int, int), (int, int))] =
    proc findall(s: string, q: string): seq[int] =
        var ret: seq[int] = @[]
        var cur = 0
        while true:
            cur = s.find(q, cur)
            if cur == -1: break
            ret &= s[0..<cur].runeLen
            cur += q.len
        return ret
    var ret: seq[((int, int), (int, int))] = @[]
    for i, chunk in p.raw.pairs:
        for res in chunk.findall(q):
            ret &= (p.translate(i, res), p.translate(i, res + q.runeLen))
    return ret
