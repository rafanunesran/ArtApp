//! Leitor mínimo de banco SQLite, só leitura, a partir dos bytes do arquivo (sem C, sem
//! dependência). Serve para os pincéis do Clip Studio (`.sut`), que são um banco SQLite. Segue o
//! formato de arquivo publicado em sqlite.org/fileformat.html: cabeçalho, árvores B de tabela
//! (páginas folha e internas), páginas de transbordo e registros com varints.
//!
//! Lê só tabelas comuns (não índices nem tabelas WITHOUT ROWID) e texto em UTF-8/UTF-16.

/// Um valor de coluna.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Int(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl Value {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Real(r) => Some(*r),
            Value::Text(t) => t.trim().parse().ok(),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::Text(t) => Some(t),
            _ => None,
        }
    }

    pub fn as_blob(&self) -> Option<&[u8]> {
        match self {
            Value::Blob(b) => Some(b),
            _ => None,
        }
    }
}

/// Uma tabela lida: nomes das colunas e as linhas.
#[derive(Clone, Debug, Default)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

impl Table {
    /// Posição da coluna `name` (sem diferenciar maiúsculas).
    pub fn col(&self, name: &str) -> Option<usize> {
        self.columns
            .iter()
            .position(|c| c.eq_ignore_ascii_case(name))
    }

    /// O valor da coluna `name` na linha `row`.
    pub fn get<'a>(&'a self, row: &'a [Value], name: &str) -> Option<&'a Value> {
        self.col(name).and_then(|i| row.get(i))
    }
}

pub struct Db<'a> {
    bytes: &'a [u8],
    page_size: usize,
    usable: usize,
    utf16: Option<bool>,
}

/// Limite de páginas visitadas por tabela (contra arquivos corrompidos com laços).
const MAX_PAGES: usize = 1 << 20;

fn be(b: &[u8]) -> u64 {
    b.iter().fold(0u64, |a, &x| (a << 8) | u64::from(x))
}

/// Varint do SQLite: até 9 bytes, big-endian, 7 bits por byte (o nono tem 8).
fn varint(b: &[u8]) -> Option<(u64, usize)> {
    let mut v = 0u64;
    for i in 0..9 {
        let x = *b.get(i)?;
        if i == 8 {
            return Some(((v << 8) | u64::from(x), 9));
        }
        v = (v << 7) | u64::from(x & 0x7f);
        if x & 0x80 == 0 {
            return Some((v, i + 1));
        }
    }
    None
}

impl<'a> Db<'a> {
    pub fn open(bytes: &'a [u8]) -> Result<Self, String> {
        if bytes.len() < 100 || !bytes.starts_with(b"SQLite format 3\0") {
            return Err("não é um banco SQLite".into());
        }
        let ps = be(&bytes[16..18]) as usize;
        let page_size = if ps == 1 { 65536 } else { ps };
        if !(512..=65536).contains(&page_size) || !page_size.is_power_of_two() {
            return Err("tamanho de página inválido".into());
        }
        let reserved = usize::from(bytes[20]);
        let utf16 = match be(&bytes[56..60]) {
            2 => Some(false),
            3 => Some(true),
            _ => None,
        };
        Ok(Self {
            bytes,
            page_size,
            usable: page_size - reserved,
            utf16,
        })
    }

    fn page(&self, n: u64) -> Option<&'a [u8]> {
        let n = usize::try_from(n).ok()?.checked_sub(1)?;
        let start = n.checked_mul(self.page_size)?;
        self.bytes.get(start..start + self.page_size)
    }

    fn text(&self, b: &[u8]) -> String {
        match self.utf16 {
            None => String::from_utf8_lossy(b).into_owned(),
            Some(big) => {
                let units: Vec<u16> = b
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| {
                        if big {
                            u16::from_be_bytes(*c)
                        } else {
                            u16::from_le_bytes(*c)
                        }
                    })
                    .collect();
                String::from_utf16_lossy(&units)
            }
        }
    }

    /// Todas as linhas (rowid, valores) da árvore de tabela que começa na página `root`.
    fn scan(&self, root: u64) -> Vec<(i64, Vec<Value>)> {
        let mut out = Vec::new();
        let mut stack = vec![root];
        let mut visited = 0usize;
        while let Some(pg) = stack.pop() {
            visited += 1;
            if visited > MAX_PAGES {
                break;
            }
            let Some(page) = self.page(pg) else {
                continue;
            };
            let off = if pg == 1 { 100 } else { 0 };
            let Some(hdr) = page.get(off..off + 12) else {
                continue;
            };
            let kind = hdr[0];
            let cells = be(&hdr[3..5]) as usize;
            let ptrs = off + if kind == 0x05 { 12 } else { 8 };
            let mut children = Vec::new();
            for c in 0..cells {
                let Some(p) = page.get(ptrs + 2 * c..ptrs + 2 * c + 2) else {
                    break;
                };
                let at = be(p) as usize;
                let Some(cell) = page.get(at..) else {
                    continue;
                };
                match kind {
                    0x05 => {
                        if let Some(child) = cell.get(..4) {
                            children.push(be(child));
                        }
                    }
                    0x0D => {
                        if let Some(row) = self.leaf_cell(cell) {
                            out.push(row);
                        }
                    }
                    _ => {}
                }
            }
            if kind == 0x05 {
                children.push(be(&hdr[8..12]));
                // Da direita para a esquerda na pilha: as linhas saem em ordem de rowid.
                stack.extend(children.into_iter().rev());
            }
        }
        out
    }

    fn leaf_cell(&self, cell: &[u8]) -> Option<(i64, Vec<Value>)> {
        let (size, a) = varint(cell)?;
        let (rowid, b) = varint(cell.get(a..)?)?;
        let size = usize::try_from(size).ok()?;
        if size > self.bytes.len() {
            return None;
        }
        let body = cell.get(a + b..)?;
        let u = self.usable;
        let x = u - 35;
        let local = if size <= x {
            size
        } else {
            let m = ((u - 12) * 32 / 255) - 23;
            let k = m + ((size - m) % (u - 4));
            if k <= x { k } else { m }
        };
        let mut payload = Vec::with_capacity(size);
        payload.extend_from_slice(body.get(..local.min(body.len()))?);
        if local < size {
            let mut next = be(body.get(local..local + 4)?);
            let mut hops = 0;
            while payload.len() < size && next != 0 && hops < MAX_PAGES {
                hops += 1;
                let page = self.page(next)?;
                next = be(page.get(..4)?);
                let take = (size - payload.len()).min(u - 4);
                payload.extend_from_slice(page.get(4..4 + take)?);
            }
        }
        Some((rowid as i64, self.record(&payload)?))
    }

    fn record(&self, p: &[u8]) -> Option<Vec<Value>> {
        let (hsize, mut h) = varint(p)?;
        let hsize = usize::try_from(hsize).ok()?;
        let mut data = hsize;
        let mut vals = Vec::new();
        while h < hsize {
            let (t, n) = varint(p.get(h..)?)?;
            h += n;
            let int = |len: usize, data: &mut usize| -> Option<Value> {
                let b = p.get(*data..*data + len)?;
                *data += len;
                // Com sinal: estende o bit mais alto.
                let shift = 64 - 8 * len as u32;
                Some(Value::Int(((be(b) << shift) as i64) >> shift))
            };
            let v = match t {
                0 => Value::Null,
                1 => int(1, &mut data)?,
                2 => int(2, &mut data)?,
                3 => int(3, &mut data)?,
                4 => int(4, &mut data)?,
                5 => int(6, &mut data)?,
                6 => int(8, &mut data)?,
                7 => {
                    let b = p.get(data..data + 8)?;
                    data += 8;
                    Value::Real(f64::from_bits(be(b)))
                }
                8 => Value::Int(0),
                9 => Value::Int(1),
                t if t >= 12 => {
                    let len = usize::try_from((t - 12) / 2).ok()?;
                    let b = p.get(data..data + len)?;
                    data += len;
                    if t % 2 == 0 {
                        Value::Blob(b.to_vec())
                    } else {
                        Value::Text(self.text(b))
                    }
                }
                _ => return None,
            };
            vals.push(v);
        }
        Some(vals)
    }

    /// As tabelas do banco: (nome, página raiz, SQL de criação).
    pub fn tables(&self) -> Vec<(String, u64, String)> {
        self.scan(1)
            .into_iter()
            .filter_map(|(_, r)| {
                if r.first()?.as_text()? != "table" {
                    return None;
                }
                let name = r.get(1)?.as_text()?.to_string();
                let root = r.get(3)?.as_f64()? as u64;
                let sql = r.get(4)?.as_text().unwrap_or_default().to_string();
                Some((name, root, sql))
            })
            .collect()
    }

    /// Lê a tabela `name` inteira (sem diferenciar maiúsculas no nome).
    pub fn table(&self, name: &str) -> Option<Table> {
        let (_, root, sql) = self
            .tables()
            .into_iter()
            .find(|(n, _, _)| n.eq_ignore_ascii_case(name))?;
        let (columns, pk) = columns_of(&sql);
        let rows = self
            .scan(root)
            .into_iter()
            .map(|(rowid, mut vals)| {
                vals.resize(columns.len().max(vals.len()), Value::Null);
                // INTEGER PRIMARY KEY fica guardado como NULL: o valor é o rowid.
                if let Some(v) = pk.and_then(|i| vals.get_mut(i))
                    && *v == Value::Null
                {
                    *v = Value::Int(rowid);
                }
                vals
            })
            .collect();
        Some(Table { columns, rows })
    }
}

/// Nomes das colunas de um `CREATE TABLE` e a posição da INTEGER PRIMARY KEY, se houver.
fn columns_of(sql: &str) -> (Vec<String>, Option<usize>) {
    let (Some(a), Some(b)) = (sql.find('('), sql.rfind(')')) else {
        return (Vec::new(), None);
    };
    let Some(inner) = sql.get(a + 1..b) else {
        return (Vec::new(), None);
    };
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0i32, 0usize);
    for (i, ch) in inner.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&inner[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&inner[start..]);
    let mut cols = Vec::new();
    let mut pk = None;
    for p in parts {
        let p = p.trim();
        let upper = p.to_ascii_uppercase();
        if ["PRIMARY", "UNIQUE", "CHECK", "FOREIGN", "CONSTRAINT"]
            .iter()
            .any(|k| upper.starts_with(k))
        {
            continue;
        }
        let name = p
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .trim_matches(|c| matches!(c, '"' | '`' | '[' | ']' | '\''));
        if name.is_empty() {
            continue;
        }
        let words: Vec<&str> = upper.split_whitespace().collect();
        if words.get(1) == Some(&"INTEGER") && upper.contains("PRIMARY KEY") {
            pk = Some(cols.len());
        }
        cols.push(name.to_string());
    }
    (cols, pk)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn varints_and_columns() {
        assert_eq!(varint(&[0x05]), Some((5, 1)));
        assert_eq!(varint(&[0x81, 0x00]), Some((128, 2)));
        let (c, pk) =
            columns_of("CREATE TABLE \"Node\" (_PW_ID INTEGER PRIMARY KEY, NodeName TEXT, x BLOB)");
        assert_eq!(c, ["_PW_ID", "NodeName", "x"]);
        assert_eq!(pk, Some(0));
    }

    #[test]
    fn rejects_garbage() {
        assert!(Db::open(b"not a database at all").is_err());
    }
}
