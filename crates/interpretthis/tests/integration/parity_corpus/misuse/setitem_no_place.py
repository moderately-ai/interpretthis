try:
    (1, 2)[0] = 5
except TypeError as e:
    print(type(e).__name__, str(e))
try:
    "abc"[0] = "x"
except TypeError as e:
    print(type(e).__name__, str(e))
class NoSet:
    pass
def make():
    return NoSet()
try:
    make()[0] = 1
except TypeError as e:
    print(type(e).__name__)
