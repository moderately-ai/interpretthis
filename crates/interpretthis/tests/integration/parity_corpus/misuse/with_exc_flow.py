def f():
    try:
        return "try"
    finally:
        print("finally runs")
print(f())
class CM:
    def __enter__(self):
        return self
    def __exit__(self, *a):
        print("exit")
        return False
def g():
    with CM():
        return "returned from with"
print(g())
def h():
    for i in range(3):
        try:
            if i == 1:
                break
        finally:
            print(f"finally {i}")
h()
assert 1 + 1 == 2
try:
    assert False, "custom message"
except AssertionError as e:
    print("assert:", e)
