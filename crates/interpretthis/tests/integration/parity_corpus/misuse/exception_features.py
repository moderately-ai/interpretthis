try:
    raise ValueError("original")
except ValueError as e:
    print(e.args)
    print(str(e))
class CustomError(Exception):
    def __init__(self, code, msg):
        super().__init__(msg)
        self.code = code
try:
    raise CustomError(404, "not found")
except CustomError as e:
    print(e.code, str(e), e.args)
try:
    raise TypeError("a", "b", "c")
except TypeError as e:
    print(e.args)
try:
    assert False, "assertion msg"
except AssertionError as e:
    print(str(e))
try:
    x = [1, 2][5]
except IndexError as e:
    print(type(e).__name__)
e = ValueError("test")
print(repr(e))
