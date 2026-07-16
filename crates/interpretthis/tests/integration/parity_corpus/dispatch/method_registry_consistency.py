def probe(value, methods, callargs):
    out = []
    for m in methods:
        try:
            fn = getattr(value, m)
        except AttributeError:
            out.append(f"{m}:NO-ATTR")
            continue
        if not callable(fn):
            out.append(f"{m}:not-callable")
            continue
        try:
            fn(*callargs.get(m, ()))
            out.append(f"{m}:ok")
        except (TypeError, ValueError, IndexError, KeyError) as e:
            out.append(f"{m}:{type(e).__name__}")
    return " ".join(out)

print(probe("Hello World", ["upper","lower","title","capitalize","swapcase","casefold",
    "strip","lstrip","rstrip","split","rsplit","splitlines","join","replace","find","rfind",
    "index","rindex","count","startswith","endswith","center","ljust","rjust","zfill",
    "expandtabs","partition","rpartition","format","format_map","encode","isalpha","isdigit",
    "isalnum","isspace","isupper","islower","istitle","isidentifier","isprintable","isascii",
    "isdecimal","isnumeric","removeprefix","removesuffix","translate"],
    {"join":(["a","b"],),"format_map":({},),"translate":({}, )}))

print(probe([1,2,3], ["append","extend","insert","remove","pop","clear","index","count",
    "sort","reverse","copy"], {"append":(4,),"extend":([5],),"insert":(0,9),"remove":(1,),
    "index":(2,)}))

print(probe({"a":1}, ["keys","values","items","get","pop","popitem","clear","update",
    "setdefault","copy","fromkeys"], {"get":("a",),"pop":("a",),"setdefault":("b",2),
    "update":({},),"fromkeys":(["x"],)}))

print(probe({1,2,3}, ["add","remove","discard","pop","clear","union","intersection",
    "difference","symmetric_difference","issubset","issuperset","isdisjoint","copy","update",
    "intersection_update","difference_update","symmetric_difference_update"],
    {"add":(4,),"remove":(1,),"discard":(2,),"union":({5},),"intersection":({1},),
     "difference":({1},),"symmetric_difference":({1},),"issubset":({1,2,3,4},),
     "issuperset":({1},),"isdisjoint":({9},),"update":({6},),"intersection_update":({1,2},),
     "difference_update":({9},),"symmetric_difference_update":({9},)}))

print(probe(b"hello", ["upper","lower","split","replace","find","index","count","startswith",
    "endswith","strip","hex","decode","title","capitalize","center","join","translate",
    "partition","removeprefix","removesuffix"],
    {"replace":(b"l",b"L"),"find":(b"o",),"index":(b"h",),"join":([b"a"],),
     "translate":(None,),"removeprefix":(b"he",),"removesuffix":(b"lo",)}))

print(probe((1,2,3), ["count","index"], {"count":(1,),"index":(2,)}))
print(probe(range(5), ["count","index"], {"count":(1,),"index":(2,)}))
print(probe(42, ["bit_length","bit_count","to_bytes","conjugate","as_integer_ratio"],
    {"to_bytes":(2,"big")}))
print(probe(3.14, ["is_integer","as_integer_ratio","hex","conjugate"], {}))
