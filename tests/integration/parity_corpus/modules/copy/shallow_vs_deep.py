# Pins: copy.copy shares nested mutables; deepcopy does not.
import copy

inner = [1, 2]
outer = [inner, 3]
shallow = copy.copy(outer)
deep = copy.deepcopy(outer)
inner.append(9)
print(shallow[0])
print(deep[0])

class Box:
    def __init__(self, v):
        self.v = v

b = Box([1])
s = copy.copy(b)
d = copy.deepcopy(b)
b.v.append(2)
print(s.v)
print(d.v)
