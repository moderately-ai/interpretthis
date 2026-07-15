# In-place operator dunders (__iadd__, __imul__, ...) and their fallback to the
# binary form, plus identity preservation.
class Acc:
    def __init__(self, v):
        self.v = v
    def __iadd__(self, o):
        self.v += o
        return self
    def __imul__(self, o):
        self.v *= o
        return self
    def __add__(self, o):
        return Acc(self.v + o)
    def __repr__(self):
        return f"Acc({self.v})"

a = Acc(10)
b = a
a += 5
print(a, a is b)
a *= 2
print(a)

# __iadd__ absent -> falls back to __add__ (new object)
class Pt:
    def __init__(self, v):
        self.v = v
    def __add__(self, o):
        return Pt(self.v + o.v)
    def __repr__(self):
        return f"Pt({self.v})"

p = Pt(1)
q = p
p += Pt(2)
print(p, p is q)

# __isub__ / __itruediv__ / __ifloordiv__ / __imod__ / __ipow__
class N:
    def __init__(self, v):
        self.v = v
    def __isub__(self, o):
        self.v -= o
        return self
    def __ifloordiv__(self, o):
        self.v //= o
        return self
    def __imod__(self, o):
        self.v %= o
        return self
    def __ipow__(self, o):
        self.v **= o
        return self
    def __repr__(self):
        return f"N({self.v})"

n = N(20)
n -= 3
n //= 2
print(n)
m = N(17)
m %= 5
m **= 3
print(m)

# in-place on builtins keeps semantics
lst = [1, 2]
lst2 = lst
lst += [3, 4]
print(lst, lst is lst2)
s = "a"
s2 = s
s += "b"
print(s, s is s2)
x = 5
x += 3
print(x)
