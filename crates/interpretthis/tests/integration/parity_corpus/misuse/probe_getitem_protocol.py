class Matrix:
    def __init__(self, data):
        self.data = data
    def __getitem__(self, key):
        if isinstance(key, tuple):
            r, c = key
            return self.data[r][c]
        return self.data[key]
    def __setitem__(self, key, value):
        r, c = key
        self.data[r][c] = value
    def __len__(self):
        return len(self.data)
m = Matrix([[1,2,3],[4,5,6]])
print(m[0])
print(m[1, 2])
m[0, 0] = 99
print(m[0])
print(len(m))
class Range2:
    def __init__(self, n):
        self.n = n
    def __getitem__(self, i):
        if i >= self.n:
            raise IndexError
        return i * i
    def __len__(self):
        return self.n
r = Range2(4)
print([r[i] for i in range(4)])
print(list(r))
print(2 in r)
class DefaultDict2:
    def __init__(self):
        self.d = {}
    def __getitem__(self, k):
        return self.d.get(k, "default")
    def __setitem__(self, k, v):
        self.d[k] = v
dd = DefaultDict2()
dd["a"] = 1
print(dd["a"], dd["missing"])
