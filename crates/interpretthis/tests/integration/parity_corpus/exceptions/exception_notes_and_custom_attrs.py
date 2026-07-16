try:
    raise ValueError("base error")
except ValueError as e:
    e.add_note("additional context")
    e.add_note("more context")
    print(e.__notes__)
    print(str(e))
class CustomError(Exception):
    pass
try:
    raise CustomError("custom")
except CustomError as e:
    print(type(e).__name__, e.args)
try:
    raise ValueError("a", "b", "c")
except ValueError as e:
    print(e.args, len(e.args))
try:
    1 / 0
except ArithmeticError as e:
    print(type(e).__name__, isinstance(e, ZeroDivisionError))
try:
    [].pop()
except IndexError as e:
    print("index:", str(e))
try:
    {}["missing"]
except KeyError as e:
    print("key:", repr(e.args[0]))
try:
    int("not a number")
except ValueError as e:
    print("valueerror caught")
try:
    "hello".nonexistent_method()
except AttributeError as e:
    print("attr error caught")
try:
    undefined_variable
except NameError as e:
    print("name error caught")
try:
    raise TypeError("type issue")
except (ValueError, TypeError) as e:
    print(f"caught {type(e).__name__}")
def chained():
    try:
        try:
            raise ValueError("original")
        except ValueError as e:
            raise RuntimeError("wrapped") from e
    except RuntimeError as e:
        return (str(e), str(e.__cause__), type(e.__cause__).__name__)
print(chained())
try:
    raise Exception("e1")
except Exception:
    try:
        raise Exception("e2")
    except Exception as e2:
        print(type(e2.__context__).__name__ if e2.__context__ else "none")
exc = ValueError("test")
exc.custom_attr = "extra"
print(exc.custom_attr)
try:
    raise StopIteration(42)
except StopIteration as e:
    print("stopvalue:", e.value)
class ValidationError(ValueError):
    def __init__(self, field, message):
        self.field = field
        super().__init__(message)
try:
    raise ValidationError("email", "invalid format")
except ValidationError as e:
    print(e.field, str(e))
except ValueError:
    print("should not reach")
errors = []
for x in [1, 0, 2, 0, 3]:
    try:
        errors.append(10 / x)
    except ZeroDivisionError:
        errors.append("inf")
print(errors)
def reraise():
    try:
        raise ValueError("first")
    except ValueError:
        raise
try:
    reraise()
except ValueError as e:
    print("reraised:", str(e))
try:
    assert False, "assertion message"
except AssertionError as e:
    print("assert:", str(e))
