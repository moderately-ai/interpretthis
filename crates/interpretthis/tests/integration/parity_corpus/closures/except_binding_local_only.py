# CPython 3.x: the `except X as e:` binding is UNBOUND when the
# handler block exits (PEP 3134) — the name `e` exists only inside
# the handler. After the handler returns, `e` is removed even if a
# variable with the same name existed before the try.
e = "outer"

def f():
    try:
        raise ValueError("test")
    except ValueError as e:
        return "caught: " + str(e)

result = f()
print(result)
# `e` at module scope should be unaffected by the inner try's
# binding — and the inner's `e` is anyway local to f.
print(e)
