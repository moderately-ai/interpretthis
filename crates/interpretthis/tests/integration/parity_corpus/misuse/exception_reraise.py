def level3():
    raise ValueError("original")
def level2():
    try:
        level3()
    except ValueError:
        raise RuntimeError("wrapped")
def level1():
    try:
        level2()
    except RuntimeError as e:
        print(f"caught: {e}")
        print(f"context: {e.__context__}")
level1()
try:
    try:
        raise KeyError("k")
    except KeyError:
        raise
except KeyError as e:
    print(f"re-raised: {e}")
def with_finally():
    try:
        raise ValueError("v")
    except ValueError:
        return "handled"
    finally:
        print("finally always")
print(with_finally())
