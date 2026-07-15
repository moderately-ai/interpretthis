def test1():
    try:
        return "try"
    finally:
        print("finally1")
print(test1())
def test2():
    try:
        raise ValueError()
    except ValueError:
        return "except"
    finally:
        print("finally2")
print(test2())
def test3():
    result = []
    for i in range(3):
        try:
            result.append(i)
            if i == 1:
                raise ValueError()
        except ValueError:
            result.append("caught")
        finally:
            result.append(f"f{i}")
    return result
print(test3())
def test4():
    try:
        try:
            raise KeyError("inner")
        finally:
            print("inner finally")
    except KeyError:
        print("outer caught")
test4()
def test5():
    try:
        return 1
    finally:
        return 2
print(test5())
