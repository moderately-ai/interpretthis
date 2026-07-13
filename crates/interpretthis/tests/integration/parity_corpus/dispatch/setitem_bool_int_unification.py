# Dict subscript-write uses the same ValueKey collapse as A1's dict-key
# unification: d[True] and d[1] target the same slot. Pins
# types::dict_set_item -> value_to_key -> ValueKey NUMERIC_TAG.
d = {}
d[1] = "int"
d[True] = "bool"                 # overwrites the int 1 entry
print(d[1])                      # "bool"
print(d[True])                   # "bool"
print(len(d))                    # 1
# List subscript-write accepts bool as int.
lst = [10, 20, 30]
lst[True] = 99
print(lst)                       # [10, 99, 30]
lst[-1] = 0
print(lst)                       # [10, 99, 0]
