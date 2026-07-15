d = {frozenset([1,2]): "a", frozenset([3]): "b"}
print(d[frozenset([2,1])])
s = {frozenset([1]), frozenset([1]), frozenset([2])}
print(len(s))
print(frozenset([1,2,3]) & frozenset([2,3,4]))
print(hash(frozenset([1,2])) == hash(frozenset([2,1])))
