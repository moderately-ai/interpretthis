text = "The quick brown fox jumps over the lazy dog"
words = text.split()
print(len(words))
print([w for w in words if len(w) > 3])
print(sorted(words, key=str.lower))
print(" ".join(w.capitalize() for w in words))
print(text.lower().count("o"))
print({w: len(w) for w in words[:3]})
csv_line = "name,age,city"
fields = csv_line.split(",")
print(fields)
print(dict(zip(fields, ["Alice", "30", "NYC"])))
lines = "line1\nline2\nline3".split("\n")
print([l.strip() for l in lines])
print("  padded  ".strip())
sentence = "hello world foo bar"
print(sentence.title())
print(sentence.replace(" ", "_"))
print("-".join(sentence.split()))
data = "key1=val1;key2=val2;key3=val3"
pairs = [p.split("=") for p in data.split(";")]
print(dict(pairs))
template = "Hello {name}, you are {age} years old"
print(template.format(name="Bob", age=25))
words2 = "apple banana apple cherry banana apple".split()
from collections import Counter
print(Counter(words2).most_common(2))
print([len(w) for w in "a bb ccc dddd".split()])
print("".join(reversed("hello")))
print("hello world".split()[::-1])
email = "user@example.com"
username, domain = email.split("@")
print(username, domain)
path = "/home/user/file.txt"
print(path.split("/")[-1])
print(path.rsplit("/", 1))
filename = "document.pdf"
name, ext = filename.rsplit(".", 1)
print(name, ext)
nums = "1,2,3,4,5"
print([int(x) for x in nums.split(",")])
print(sum(int(x) for x in nums.split(",")))
markdown = "# Title\n## Subtitle\nContent"
headers = [l for l in markdown.split("\n") if l.startswith("#")]
print(headers)
print("CamelCaseString".replace("C", " C").strip())
words3 = ["cat", "dog", "bird"]
print(", ".join(words3))
print(" | ".join(f"{w}({len(w)})" for w in words3))
text2 = "the cat sat on the mat"
word_positions = {}
for i, word in enumerate(text2.split()):
    word_positions.setdefault(word, []).append(i)
print(sorted(word_positions.items()))
print("Hello World".swapcase())
print("hello".ljust(10, ".") + "end")
print("Line 1\nLine 2\nLine 3".count("\n"))
