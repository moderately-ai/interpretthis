def append_to(x, lst=[]):
    lst.append(x)
    return lst
print(append_to(1))
print(append_to(2))
print(append_to(3, []))
print(append_to(4))
def cache_dict(k, v, d={}):
    d[k] = v
    return d
print(cache_dict("a", 1))
print(cache_dict("b", 2))
