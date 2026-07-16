print("x{}".format("y"))          # direct call
f = "x{}".format                  # bound method
print(f("z"))                     # bound call
print(getattr("a{}", "format")("b"))
g = "{0}-{1}".format
print(g("p", "q"))
print(list(map("n={}".format, [1, 2, 3])))
fm = "{a}".format_map
print(fm({"a": 5}))
# other potentially-async bound methods
print("a,b".split.__name__ if hasattr("a,b".split, "__name__") else "no-name")
j = ",".join
print(j(["1", "2", "3"]))
e = "café".encode
print(e("utf-8"))
t = "abc".translate
print(t(str.maketrans("a", "X")))
