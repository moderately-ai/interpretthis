print(list(map(lambda x: x*2, filter(lambda x: x > 2, range(6)))))
print(sorted(filter(lambda x: x % 2, range(10)), reverse=True))
print([x for x in map(str, range(3))])
print(sum(map(len, ["a", "bb", "ccc"])))
print(list(zip(map(str.upper, "abc"), range(3))))
print(max(map(abs, [-5, 3, -8, 2])))
data = [{"v": 3}, {"v": 1}, {"v": 2}]
print(list(map(lambda d: d["v"], sorted(data, key=lambda d: d["v"]))))
print(list(filter(None, map(lambda x: x if x % 2 else 0, range(5)))))
print(dict(zip("abc", map(lambda x: x**2, range(3)))))
nested = [[1, 2], [3, 4], [5, 6]]
print(list(map(sum, nested)))
print(sorted(set(map(lambda x: x % 3, range(10)))))
print("".join(filter(str.isalpha, "a1b2c3")))
