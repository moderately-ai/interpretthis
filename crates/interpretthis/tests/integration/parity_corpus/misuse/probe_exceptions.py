try:
    raise ValueError("bad")
except ValueError as e:
    print(type(e).__name__, e.args)
try:
    [][5]
except IndexError as e:
    print("index", str(e))
try:
    {}["missing"]
except KeyError as e:
    print("key", repr(e.args))
try:
    int("abc")
except ValueError as e:
    print("valueerror")
try:
    1/0
except ZeroDivisionError as e:
    print(str(e))
try:
    raise RuntimeError("a", "b", "c")
except RuntimeError as e:
    print(e.args)
try:
    raise StopIteration(42)
except StopIteration as e:
    print(e.value)
