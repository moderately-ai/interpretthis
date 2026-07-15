class CI:
    def __init__(self, s): self.s = s.lower()
    def __eq__(self, o): return isinstance(o, CI) and self.s == o.s
    def __hash__(self): return hash(self.s)
    def __repr__(self): return f"CI({self.s})"
d = {CI("Hello"): 1}
print(d[CI("HELLO")])
print(CI("hello") in d)
d[CI("WORLD")] = 2
print(len(d))
d[CI("hello")] = 10
print(d[CI("HeLLo")], len(d))
s = {CI("A"), CI("a"), CI("B")}
print(len(s))
print(CI("b") in s)
