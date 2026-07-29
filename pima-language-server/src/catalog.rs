pub struct Member {
    pub name: &'static str,
    pub signature: &'static str,
}

pub fn namespace_members(namespace: &str) -> Option<&'static [Member]> {
    match namespace {
        "Math" => Some(&MATH),
        "String" => Some(&STRING),
        "List" => Some(&LIST),
        "Types" => Some(&TYPES),
        "Console" => Some(&CONSOLE),
        "Logic" => Some(&LOGIC),
        _ => None,
    }
}

pub const NAMESPACES: &[&str] = &["Console", "List", "Logic", "Math", "String", "Types"];

const MATH: [Member; 18] = [
    Member {
        name: "E",
        signature: "Math.E: float",
    },
    Member {
        name: "PI",
        signature: "Math.PI: float",
    },
    Member {
        name: "pow",
        signature: "Math.pow(base, exponent)",
    },
    Member {
        name: "less_or_equal",
        signature: "Math.less_or_equal(left, right)",
    },
    Member {
        name: "greater_or_equal",
        signature: "Math.greater_or_equal(left, right)",
    },
    Member {
        name: "increment",
        signature: "Math.increment(value)",
    },
    Member {
        name: "decrement",
        signature: "Math.decrement(value)",
    },
    Member {
        name: "range",
        signature: "Math.range(begin, end)",
    },
    Member {
        name: "absolute",
        signature: "Math.absolute(value)",
    },
    Member {
        name: "minimum",
        signature: "Math.minimum(left, right)",
    },
    Member {
        name: "maximum",
        signature: "Math.maximum(left, right)",
    },
    Member {
        name: "clamp",
        signature: "Math.clamp(value, minimum, maximum)",
    },
    Member {
        name: "sum",
        signature: "Math.sum(list)",
    },
    Member {
        name: "product",
        signature: "Math.product(list)",
    },
    Member {
        name: "average",
        signature: "Math.average(list)",
    },
    Member {
        name: "div",
        signature: "Math.div(left, right)",
    },
    Member {
        name: "mod",
        signature: "Math.mod(left, right)",
    },
    Member {
        name: "int",
        signature: "Math.int(value)",
    },
];

const STRING: [Member; 17] = [
    Member {
        name: "concat",
        signature: "String.concat(values...)",
    },
    Member {
        name: "length",
        signature: "String.length(value)",
    },
    Member {
        name: "slice",
        signature: "String.slice(value, start, end)",
    },
    Member {
        name: "chars",
        signature: "String.chars(value)",
    },
    Member {
        name: "code_point",
        signature: "String.code_point(value)",
    },
    Member {
        name: "from_code_point",
        signature: "String.from_code_point(value)",
    },
    Member {
        name: "from",
        signature: "String.from(value)",
    },
    Member {
        name: "string",
        signature: "String.string(value)",
    },
    Member {
        name: "lower",
        signature: "String.lower(value)",
    },
    Member {
        name: "upper",
        signature: "String.upper(value)",
    },
    Member {
        name: "trim",
        signature: "String.trim(value)",
    },
    Member {
        name: "contains?",
        signature: "String.contains?(value, search)",
    },
    Member {
        name: "starts_with?",
        signature: "String.starts_with?(value, prefix)",
    },
    Member {
        name: "ends_with?",
        signature: "String.ends_with?(value, suffix)",
    },
    Member {
        name: "replace",
        signature: "String.replace(value, from, to)",
    },
    Member {
        name: "split",
        signature: "String.split(value, separator)",
    },
    Member {
        name: "join",
        signature: "String.join(values, separator)",
    },
];

const LIST: [Member; 14] = [
    Member {
        name: "push",
        signature: "List.push(list, value)",
    },
    Member {
        name: "append",
        signature: "List.append(list, value)",
    },
    Member {
        name: "head",
        signature: "List.head(list)",
    },
    Member {
        name: "rest",
        signature: "List.rest(list)",
    },
    Member {
        name: "empty?",
        signature: "List.empty?(list)",
    },
    Member {
        name: "reverse",
        signature: "List.reverse(list)",
    },
    Member {
        name: "foreach",
        signature: "List.foreach(list, block)",
    },
    Member {
        name: "map",
        signature: "List.map(callable, list)",
    },
    Member {
        name: "length",
        signature: "List.length(list)",
    },
    Member {
        name: "contains?",
        signature: "List.contains?(list, value)",
    },
    Member {
        name: "fold",
        signature: "List.fold(callable, initial, list)",
    },
    Member {
        name: "filter",
        signature: "List.filter(predicate, list)",
    },
    Member {
        name: "any?",
        signature: "List.any?(predicate, list)",
    },
    Member {
        name: "all?",
        signature: "List.all?(predicate, list)",
    },
];

const TYPES: [Member; 2] = [
    Member {
        name: "of",
        signature: "Types.of(value)",
    },
    Member {
        name: "is?",
        signature: "Types.is?(value, type)",
    },
];

const CONSOLE: [Member; 1] = [Member {
    name: "println",
    signature: "Console.println(values...)",
}];

const LOGIC: [Member; 2] = [
    Member {
        name: "not",
        signature: "Logic.not(value)",
    },
    Member {
        name: "select",
        signature: "Logic.select(predicate, when_true, when_false)",
    },
];
