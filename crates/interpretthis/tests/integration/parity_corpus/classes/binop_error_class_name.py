class NI:
    def __add__(self, o): return NotImplemented
    def __sub__(self, o): return NotImplemented
try:
    NI() + NI()
except TypeError as e:
    print(str(e))
try:
    NI() + 5
except TypeError as e:
    print(str(e))
try:
    5 - NI()
except TypeError as e:
    print(str(e))
try:
    [1, 2] + NI()
except TypeError as e:
    print(str(e))
try:
    NI() * 3
except TypeError as e:
    print(str(e))
