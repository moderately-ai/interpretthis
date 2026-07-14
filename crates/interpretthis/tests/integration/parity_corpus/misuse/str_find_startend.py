s = "abcabcabc"
print(s.find("a", 1))
print(s.find("a", 1, 3))
print(s.count("a", 2))
print(s.index("b", 3))
print(s.rfind("c", 0, 5))
print("hello".startswith("lo", 3))
print("hello".startswith(("he", "xx")))
