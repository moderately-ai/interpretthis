# Subscript-read (`__getitem__`) dispatch over every builtin container.
# Pins types::dispatch_getitem -> per-type get_item_slot routing for
# list / tuple / str / bytes / dict / range. Negative indices, bool-as-int,
# and KeyError on a dict miss all flow through one entry point now.
print([10, 20, 30][0])
print([10, 20, 30][-1])
print((1, 2, 3)[1])
print("abc"[0])
print("abc"[-1])
print(b"abc"[0])                 # integer byte value 97
print(b"abc"[-1])                # 99
print({"a": 1, "b": 2}["a"])
print({1: "x", 2: "y"}[2])
print(range(10)[3])
print(range(0, 10, 2)[2])        # 4
print(range(10, 0, -1)[0])       # 10
print([10, 20, 30][True])        # bool indexes like int -> 20
try:
    [1, 2, 3][10]
except IndexError as e:
    print("IndexError")
try:
    {"a": 1}["missing"]
except KeyError as e:
    print("KeyError")
