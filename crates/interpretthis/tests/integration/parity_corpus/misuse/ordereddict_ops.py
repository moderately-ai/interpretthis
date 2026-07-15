from collections import OrderedDict
od = OrderedDict([("a", 1), ("b", 2), ("c", 3)])
print(list(od.items()))
od.move_to_end("a")
print(list(od.keys()))
od.move_to_end("c", last=False)
print(list(od.keys()))
print(od.popitem())
print(od.popitem(last=False))
od2 = OrderedDict(x=1, y=2)
print(list(od2.items()))
od3 = OrderedDict()
od3["z"] = 26
od3["a"] = 1
print(list(od3.keys()))
print(od == OrderedDict([("b", 2)]))
print(len(OrderedDict([("k", "v")])))
