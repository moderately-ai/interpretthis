class M:
    __hash__ = None

class E:
    def __eq__(self, o):
        return True

for cls, label in [(M, "M"), (E, "E")]:
    try:
        {cls(): 1}
    except TypeError:
        print(f"dict-literal {label} unhashable")
    try:
        d = {}
        d[cls()] = 1
    except TypeError:
        print(f"dict-setitem {label} unhashable")
    try:
        hash(cls())
    except TypeError:
        print(f"hash() {label} unhashable")
    try:
        cls() in {1, 2}
    except TypeError:
        print(f"in-set {label} unhashable")
