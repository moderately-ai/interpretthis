# CPython's arity TypeErrors name the callable and list all missing positional
# arguments (with count and 'and'/', and'), and round()/complex() accept their
# keyword arguments.
def te(f):
    try:
        f()
    except TypeError as e:
        return str(e)

def one(a):
    return a
def two(a, b):
    return a
def three(a, b, c):
    return a
def mixed(a, b, c=3):
    return a

print(te(lambda: one()))
print(te(lambda: two(1)))
print(te(lambda: two()))
print(te(lambda: three()))
print(te(lambda: three(1)))
print(te(lambda: mixed(1)))
print(te(lambda: one(1, 2)))
print(te(lambda: two(1, 2, 3)))

class C:
    def __init__(self, x, y):
        self.x = x
    def method(self, a, b):
        return a
print(te(lambda: C(1)))
print(te(lambda: C(1).method(2)) if False else te(lambda: C(1, 2).method(5)))

f = lambda a, b: a + b
print(te(lambda: f(1)))

print(round(3.14159, ndigits=2))
print(round(3.14159, 3))
print(round(2.5), round(1234, ndigits=-2))
print(complex(real=3, imag=4), complex(imag=5), complex(real=2))
