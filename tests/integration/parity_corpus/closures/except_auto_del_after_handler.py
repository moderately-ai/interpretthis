# PEP 3134: the target of an `except X as e:` clause is automatically
# `del`'d at the end of the except block. Reading `e` after the
# handler must raise NameError, even within the same function.
def f():
    try:
        raise ValueError("test")
    except ValueError as e:
        msg = "caught: " + str(e)
    # `e` was del'd at the end of the except block.
    try:
        return msg, e
    except NameError:
        return msg, "unbound"

print(f())
