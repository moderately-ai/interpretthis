# User __eq__/__hash__ that diverge from field-by-field structural equality.
# Pins A1: dict keys, set membership, list count/in use the dunder path.
class CaseFold:
    def __init__(self, s):
        self.s = s
    def __eq__(self, other):
        return isinstance(other, CaseFold) and self.s.lower() == other.s.lower()
    def __hash__(self):
        return hash(self.s.lower())

a = CaseFold("AbC")
b = CaseFold("abc")
print(a == b)
print(a != b)
print([a].count(b))
print(b in [a])
d = {a: 1}
print(d[b])
print(b in d)
s = {a, b}
print(len(s))
