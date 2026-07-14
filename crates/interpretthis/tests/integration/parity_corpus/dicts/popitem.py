# dict.popitem() removes and returns the last inserted pair (LIFO); empty raises.
d = {"a": 1, "b": 2, "c": 3}
print(d.popitem())
print(d.popitem())
print(d)
try:
    {}.popitem()
except KeyError as e:
    print("KeyError:", str(e))
