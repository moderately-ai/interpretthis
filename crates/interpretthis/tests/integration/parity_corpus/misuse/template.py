from string import Template
t = Template("$name is $age years old")
print(t.substitute(name="Alice", age=30))
print(t.substitute({"name": "Bob", "age": 25}))
t2 = Template("${greeting}, World! Cost: $$5")
print(t2.substitute(greeting="Hello"))
t3 = Template("Hi $who, $missing")
print(t3.safe_substitute(who="X"))
try:
    t3.substitute(who="X")
except KeyError as e:
    print("keyerror", e)
print(t.template)
