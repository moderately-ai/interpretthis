# Exception.add_note (3.11+) and __notes__.
try:
    e = ValueError("bad")
    e.add_note("note 1")
    e.add_note("note 2")
    raise e
except ValueError as ex:
    print(ex.__notes__, str(ex))

# args tuple and multiple args
e2 = ValueError("a", "b", "c")
print(e2.args, str(e2))

# exception with no args
e3 = ValueError()
print(e3.args, repr(e3))

# custom exception hierarchy with attributes
class AppError(Exception):
    pass

class NotFound(AppError):
    def __init__(self, resource):
        super().__init__(f"{resource} not found")
        self.resource = resource

try:
    raise NotFound("user")
except AppError as e:
    print(type(e).__name__, e.resource, str(e))
    print(isinstance(e, AppError), isinstance(e, Exception))

# raise from with note
try:
    try:
        1 / 0
    except ZeroDivisionError as z:
        err = RuntimeError("wrapped")
        err.add_note("context info")
        raise err from z
except RuntimeError as e:
    print(str(e), e.__notes__, type(e.__cause__).__name__)

# BaseException subclasses
print(issubclass(ValueError, Exception), issubclass(KeyboardInterrupt, Exception))
print(issubclass(ArithmeticError, Exception), issubclass(ZeroDivisionError, ArithmeticError))
