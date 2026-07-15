class Num:
    def __init__(self, v):
        self.v = v
    def __lt__(self, o):
        return self.v < o.v
    def __eq__(self, o):
        return self.v == o.v
a, b = Num(1), Num(2)
print(a < b)
print(b > a)
try:
    a <= b
except TypeError as e:
    print(type(e).__name__, str(e))
print(min([Num(3), Num(1), Num(2)], key=lambda n: n.v).v)
print(max(Num(1), Num(5), Num(3), key=lambda n: n.v).v)
