class AppError(Exception):
    def __init__(self, code, detail):
        self.code = code
        self.detail = detail
        super().__init__(code, detail)
try:
    raise AppError(404, "not found")
except AppError as e:
    print(e.code, e.detail)
    print(str(e))
    print(repr(e))
    print(e.args)

class NoSuper(Exception):
    def __init__(self, x):
        self.x = x
try:
    raise NoSuper(9)
except NoSuper as e:
    print(e.x, repr(str(e)), e.args)

class Wrapped(Exception):
    def __init__(self, msg):
        super().__init__(msg)
        self.tag = "W"
def go():
    try:
        raise ValueError("inner")
    except ValueError as e:
        raise Wrapped("outer") from e
try:
    go()
except Wrapped as e:
    print(e.tag, str(e), type(e.__cause__).__name__)

class Level2(AppError):
    pass
try:
    raise Level2(500, "boom")
except AppError as e:
    print("subclass", e.code, e.detail)
