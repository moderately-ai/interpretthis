try:
    raise ValueError("v")
except (TypeError, ValueError) as e:
    print("caught", type(e).__name__, str(e))
try:
    d = {}
    d["missing"]
except KeyError as e:
    print("key", str(e))
try:
    [1, 2][10]
except IndexError as e:
    print("index")
try:
    1 / 0
except ArithmeticError:
    print("arith")
try:
    int("notanumber")
except ValueError:
    print("valueerror")
try:
    raise RuntimeError("a")
except Exception as e:
    print("base caught", type(e).__name__)
finally:
    print("finally runs")
def chain():
    try:
        raise ValueError("original")
    except ValueError as e:
        raise TypeError("wrapped") from e
try:
    chain()
except TypeError as e:
    print(type(e.__cause__).__name__)
class MyError(Exception):
    def __init__(self, code):
        self.code = code
        super().__init__(f"error {code}")
try:
    raise MyError(42)
except MyError as e:
    print(e.code, str(e))
