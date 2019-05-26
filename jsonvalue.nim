import json
import util
import streams
import tables
import unicode
import listformat

let
    fgString = newColorSpec(76, 2, false)
    fgKeyword = newColorSpec(214, 1, false)
    fgKey = newColorSpec(183, 5, false)
    fgMuted = newColorSpec(244, 4, false)

type JsonValue* = ref object
    key: string
    value: JsonNode
    idx: int
    parent: JsonValue
    depth: int

proc newJsonRoot*(input: Stream): JsonValue =
    let value = input.parseJson
    JsonValue(key: "", value: value, idx: 0, parent: nil, depth: 0)

proc fmtstr(s: string): FmtCmd =
    var parts: seq[FmtCmd] = @[]
    var cur: string = ""
    for c in s.runes:
        case c.int32
        of 0..8, 11..31, 127:
            parts &= @[flit(cur), fexclude(fnobreak(fcolor(fgKeyword, flit("^" & ((c.ord + 64) mod 128).chr))))]
            cur = ""
        else:
            cur &= c.toUTF8
    if cur.len > 0: parts &= flit(cur)
    return fcat(parts)

proc fmtkey(v: JsonValue): FmtCmd =
    if v.parent == nil: return fcolor(fgMuted, flit("root")) # "•"
    case v.parent.value.kind
    of JObject: return fcolor(fgKey, fmtstr(v.key))
    of JArray: return fexclude(fcolor(fgMuted, fmtstr(v.key)))
    else: assert(false)

proc fmtval(v: JsonValue): FmtCmd =
    case v.value.kind
    of JInt: fcolor(fgKeyword, flit($v.value.num))
    of JFloat: fcolor(fgKeyword, flit($v.value.fnum))
    of JBool: fcolor(fgKeyword, flit($v.value.bval))
    of JNull: fcolor(fgKeyword, flit("null"))
    of JString: fcolor(fgString, fmtstr(v.value.str))
    of JObject: fexclude(fcolor(fgKeyword, flit(if v.value.fields.len == 0: "{ }" else: "{...}")))
    of JArray: fexclude(fcolor(fgKeyword, flit(if v.value.elems.len == 0: "[ ]" else: "[...]")))

proc placeholder*(v: JsonValue): FmtCmd =
    return v.fmtkey

proc content*(v: JsonValue): FmtCmd =
    if v.parent == nil: return v.fmtval
    return fcat(@[v.fmtkey, fexclude(fcolor(fgMuted, flit(": "))), v.fmtval])

proc expandable*(val: JsonValue): bool =
    case val.value.kind
    of JObject, JArray: true
    else: false

proc children*(val: JsonValue): seq[JsonValue] =
    var ret: seq[JsonValue] = @[]
    case val.value.kind
    of JObject:
        for k, v in val.value.fields: ret &= JsonValue(key: k, value: v, idx: ret.len, parent: val, depth: val.depth + 1)
    of JArray:
        for i, v in val.value.elems: ret &= JsonValue(key: $i, value: v, idx: ret.len, parent: val, depth: val.depth + 1)
    else: discard
    return ret

proc index*(v: JsonValue): int =
    return v.idx

# FIXME WHY does this break compile?
#proc `$`*(v: JsonValue): string =
#    return v.key
