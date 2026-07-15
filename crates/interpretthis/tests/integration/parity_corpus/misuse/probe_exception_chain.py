try:
    try:
        1/0
    except ZeroDivisionError as e:
        raise ValueError("wrapped") from e
except ValueError as e:
    print(type(e).__name__, str(e))
    print(type(e.__cause__).__name__)
try:
    raise KeyError("k")
except (KeyError, IndexError) as e:
    print("caught", type(e).__name__)
def risky():
    raise RuntimeError("boom")
try:
    risky()
except RuntimeError as e:
    print("got", str(e))
finally:
    print("cleanup")
class MyError(Exception):
    def __init__(self, code):
        self.code = code
        super().__init__(f"error {code}")
try:
    raise MyError(404)
except MyError as e:
    print(e.code, str(e))
try:
    assert False, "assertion msg"
except AssertionError as e:
    print("assert", str(e))
