def f():
    try:
        return 1
    finally:
        print("finally")
print(f())
def g():
    try:
        return 1
    finally:
        return 2
print(g())
def h():
    for i in range(3):
        try:
            if i == 1:
                break
        finally:
            print("iter", i)
    return "done"
print(h())
