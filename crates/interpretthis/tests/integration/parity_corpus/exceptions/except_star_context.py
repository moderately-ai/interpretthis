# An exception raised inside an except* handler chains the matched group as its
# implicit __context__.
try:
    try:
        raise ValueError("v")
    except* ValueError:
        raise RuntimeError("in handler")
except RuntimeError as e:
    print(type(e.__context__).__name__)
    print(e.__cause__ is None)
