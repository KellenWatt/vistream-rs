# Vistream Client Protocol

### Notation used in diagrams
```
one byte:
+--------+
|        |
+--------+

a variable number of bytes:
+========+
|        |
+========+
```

### Client to Server

#### Start

A single byte containing the value `0x01`

```
+--------+
|        |
+--------+
```
