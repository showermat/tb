
import tables
import util

type Trie = ref object
    children: Table[char, Trie]

proc newTrie*(): Trie =
    Trie(children: initTable[char, Trie]())

proc register*(t: Trie, path: string) =
    if path == "": return
    let child = t.children.mgetOrPut(path[0], newTrie())
    child.register(path[1..^1])

proc newTrie*(paths: seq[string]): Trie =
    let ret = newTrie()
    for path in paths: ret.register(path)
    return ret

proc readchars*(t: Trie, f: File, path: string = ""): string =
    if t.children.len == 0: return path
    let next = readchar(f)
    # TODO One-second timeout
    if not (next in t.children): return ""
    return t.children[next].readchars(f, path & next)
