def parse(s):
    try:
        n = int(s)
    except ValueError:
        return "invalid"
    else:
        return n * 2
    finally:
        pass
print(parse("21"))
print(parse("abc"))
results = []
for x in ["1", "a", "3"]:
    try:
        results.append(int(x))
    except ValueError:
        results.append(-1)
print(results)
try:
    raise TypeError("t")
except ValueError:
    print("value")
except TypeError:
    print("type")
except Exception:
    print("other")
count = 0
try:
    for i in range(5):
        if i == 3:
            break
        count += 1
finally:
    print(f"count={count}")
try:
    x = 1
except:
    pass
else:
    print("no exception")
