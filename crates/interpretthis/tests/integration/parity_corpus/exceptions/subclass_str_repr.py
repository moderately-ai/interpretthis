class AppError(Exception):
    def __init__(self, msg, code):
        super().__init__(msg)
        self.code = code
    def __str__(self): return f"[{self.code}] {self.args[0]}"
try:
    raise AppError("failed", 500)
except AppError as e:
    print(str(e), e.code, e.args)
class ValidationError(ValueError):
    pass
try:
    raise ValidationError("bad input")
except ValueError as e:
    print(type(e).__name__, isinstance(e, ValueError), str(e))
try:
    raise KeyError("missing")
except KeyError as e:
    print(repr(e), str(e))
try:
    {}["x"]
except KeyError as e:
    print("caught", e.args)
try:
    [1,2][10]
except IndexError as e:
    print("index", str(e))
try:
    int("abc")
except ValueError as e:
    print("value error caught")
