for exc_call, name in [
    (lambda: [][0], "IndexError"),
    (lambda: {}["k"], "KeyError"),
    (lambda: 1/0, "ZeroDivisionError"),
    (lambda: int("x"), "ValueError"),
    (lambda: None.foo, "AttributeError"),
    (lambda: undefined_var, "NameError"),
    (lambda: "a" + 1, "TypeError"),
    (lambda: [].remove(5), "ValueError"),
]:
    try:
        exc_call()
    except Exception as e:
        print(type(e).__name__ == name, type(e).__name__)
print(issubclass(ZeroDivisionError, ArithmeticError))
print(issubclass(KeyError, LookupError))
print(isinstance(ValueError(), Exception))
