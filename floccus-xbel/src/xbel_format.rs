// std
use std::collections::VecDeque;
use std::fmt::{Display, Formatter};
use std::io::{BufReader, Write};
use std::path::Path;
// third-party
use quick_xml::de::from_reader;
use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::writer::Writer;
use serde::{Deserialize, Serialize};
use thiserror::Error;
// internal

/// The title of a `Bookmark` or `Folder`
#[derive(Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, rename = "lowercase")]
pub struct Title {
    #[serde(rename = "$text")]
    pub text: String,
}

impl Title {
    fn new(title: &str) -> Self {
        Self {
            text: title.to_string(),
        }
    }
}

/// A Bookmark aka a `Title` and usually a www url
#[derive(Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, rename = "lowercase")]
pub struct Bookmark {
    #[serde(rename = "@href")]
    pub href: String,
    #[serde(rename = "@id")]
    pub id: String,
    pub title: Title,
}

impl Bookmark {
    fn new(id: &str, url: &str, title: &str) -> Self {
        Self {
            href: url.to_string(),
            id: id.to_string(),
            title: Title::new(title),
        }
    }
}

/// An enum that is either a `Folder` or a `Bookmark`. See `XbelIterator` or `XbelNestingIterator`.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum XbelItem {
    #[serde(rename = "folder")]
    Folder(Folder),
    #[serde(rename = "bookmark")]
    Bookmark(Bookmark),
}

impl XbelItem {
    pub(crate) fn new_bookmark(id: &str, url: &str, title: &str) -> Self {
        Self::Bookmark(Bookmark::new(id, url, title))
    }
}

impl XbelItem {
    /// Get title of Bookmark or Folder
    pub fn get_title(&self) -> &Title {
        match self {
            XbelItem::Folder(f) => &f.title,
            XbelItem::Bookmark(b) => &b.title,
        }
    }
    /// Get id of Bookmark or Folder
    pub fn get_id(&self) -> &String {
        match self {
            XbelItem::Folder(f) => &f.id,
            XbelItem::Bookmark(b) => &b.id,
        }
    }
    
    /// Get the url of a Bookmark or None if it's a Folder 
    pub fn get_url(&self) -> Option<&String> {
        match self {
            XbelItem::Folder(_f) => None,
            XbelItem::Bookmark(b) => Some(&b.href),
        }
    }
}

/// A Folder that contains folders and bookmarks
#[derive(Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, rename = "lowercase")]
pub struct Folder {
    #[serde(rename = "@id")]
    pub id: String,
    pub title: Title,
    #[serde(rename = "$value")]
    pub items: Vec<XbelItem>,
}

impl Folder {
    #[allow(dead_code)]
    fn new(id: &str, title: &str, items: Option<Vec<XbelItem>>) -> Self {
        Self {
            id: id.to_string(),
            title: Title::new(title),
            items: items.unwrap_or_default(),
        }
    }
}

/// A struct to search inside a `Xbel` struct
#[derive(Debug, Clone)]
pub enum XbelPath {
    Root,
    Id(u64),
    Path(String),
}

impl Display for XbelPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            XbelPath::Root => write!(f, "root"),
            XbelPath::Id(id) => write!(f, "id = {}", id),
            XbelPath::Path(p) => write!(f, "path = {}", p),
        }
    }
}

/// Struct resulting from parsing a Xbel file
#[derive(Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, rename = "xbel")]
pub struct Xbel {
    #[serde(rename = "@version")]
    version: String,
    #[serde(rename = "$value")]
    pub(crate) items: Vec<XbelItem>,
}

impl Xbel {
    #[allow(dead_code)]
    fn new(items: Option<Vec<XbelItem>>) -> Self {
        Self {
            version: "1.0".to_string(),
            items: items.unwrap_or_default(),
        }
    }

    pub(crate) fn get_highest_id(&self) -> u64 {
        
        let it = XbelIterator::new(self);
        it.fold(0, |mut acc, x| {
            let id = x.get_id().parse::<u64>().unwrap();
            if id > acc {
                acc = id;
            }
            acc
        })
    }

    /// Returns the mutable list of XbelItem containing the searched item (`XbelPath`)
    pub fn get_items_mut(&mut self, path: &XbelPath) -> Option<(usize, &mut Vec<XbelItem>)> {
        match path {
            XbelPath::Root => Some((0, &mut self.items)),
            XbelPath::Id(id) => {
                // All the Vec<XbelItem> to check (in order to find the id)
                let mut to_process = VecDeque::from([&mut self.items]);
                while let Some(items) = to_process.pop_front() {
                    let found = items.iter().enumerate().find_map(|(item_index, item)| {
                        let item_id = item.get_id().parse::<u64>().unwrap();
                        if item_id == *id {
                            Some(item_index)
                        } else {
                            None
                        }
                    });
                    if let Some(item_index) = found {
                        return Some((item_index, items));
                    }

                    // If not found yet, update to_process
                    for item in items.iter_mut() {
                        match item {
                            XbelItem::Folder(ref mut f) => {
                                to_process.push_back(&mut f.items);
                            }
                            XbelItem::Bookmark(_) => {}
                        }
                    }
                }

                None
            }
            XbelPath::Path(s) => {
                let path_split = s.split('/').collect::<Vec<&str>>();
                // Safe to unwrap()
                let mut path_split_index = 0;

                // All the Vec<XbelItem> to check (in order to find the id)
                let mut to_process = VecDeque::from([&mut self.items]);

                while let Some(items) = to_process.pop_front() {
                    let found = items.iter().enumerate().find_map(|(item_index, item)| {
                        if item.get_title().text == path_split[path_split_index] {
                            Some(item_index)
                        } else {
                            None
                        }
                    });
                    if let Some(item_index) = found {
                        if path_split_index == path_split.len() - 1 {
                            return Some((item_index, items));
                        } else {
                            path_split_index += 1;
                        }
                    }

                    // Not (all) found yet, update to_process
                    for item in items.iter_mut() {
                        match item {
                            XbelItem::Folder(ref mut f) => {
                                to_process.push_back(&mut f.items);
                            }
                            XbelItem::Bookmark(_) => {}
                        }
                    }
                }

                None
            }
        }
    }

    /// Serialize to string
    /// 
    /// This is the recommended way to serialize a Xbel that will be compatible with Floccus. Using 
    /// the derive implementation should result in a valid xml file but missing some information 
    /// and proper indentation.
    pub fn to_string(&self) -> String {
        // Note:
        // quick_xml 0.37 (when using the derive feature) can serialize comment (for highest_id)
        
        let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);
        let comment = format!(
            "- highestId :{}: for Floccus bookmark sync browser extension ",
            self.get_highest_id()
        );
        writer
            .write_event(Event::Comment(BytesText::new(comment.as_str())))
            .expect("writing comment should succeed");
        writer
            .write_event(Event::Text(BytesText::new("\n\n")))
            .expect("writing empty should succeed");

        for item in self.items.iter() {
            write_xbel_item(&mut writer, item);
        }

        let result_ = writer.into_inner();

        const XML_HEADER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE xbel PUBLIC "+//IDN python.org//DTD XML Bookmark Exchange Language 1.0//EN//XML" "http://pyxml.sourceforge.net/topics/dtds/xbel.dtd">
"#;
        const XBEL_START: &str = "<xbel version=\"1.0\">\n";
        const XBEL_END: &str = "\n</xbel>";

        let mut result = String::with_capacity(
            result_.len() + XML_HEADER.len() + XBEL_START.len() + XBEL_END.len(),
        );

        result.push_str(XML_HEADER);
        result.push_str(XBEL_START);
        result.push_str(String::from_utf8(result_).unwrap().as_str());
        result.push_str(XBEL_END);

        result
    }

    /// Create a new bookmark for this Xbel using the correct id (highest id + 1).
    pub fn new_bookmark(&self, url: &str, title: &str) -> XbelItem {
        let highest_id = self.get_highest_id();

        XbelItem::new_bookmark((highest_id + 1).to_string().as_str(), url, title)
    }
    
    /// Parse a file into a Xbel
    pub fn try_from_file<T: AsRef<Path>>(path: T) -> Result<Xbel, XbelError> {
        let xbel_ = std::fs::File::open(path)?;
        let xbel: Xbel = from_reader(BufReader::new(xbel_))?;
        Ok(xbel)
    }

    /// Write Xbel to a file
    pub fn try_to_file<T: AsRef<Path>>(&self, file_path: T) -> Result<(), XbelError> {
        let mut f = std::fs::File::options()
            .write(true)
            .truncate(true)
            .open(file_path)?;
        let buffer = self.to_string();
        f.write_all(buffer.as_bytes())?;
        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum XbelError {
    #[error("Error while reading Xbel file: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Cannot parse Xbel file: {0}")]
    XbelReadError(#[from] quick_xml::de::DeError),
}

fn write_xbel_item<W: std::io::Write>(writer: &mut Writer<W>, item: &XbelItem) {
    match item {
        XbelItem::Folder(f) => {
            writer
                .write_event(Event::Start(
                    BytesStart::new("folder").with_attributes([("id", f.id.to_string().as_str())]),
                ))
                .expect("writing start tag should succeed");
            writer
                .write_event(Event::Start(BytesStart::new("title")))
                .expect("writing start tag should succeed");
            writer
                .write_event(Event::Text(BytesText::new(f.title.text.as_str())))
                .expect("writing start tag should succeed");
            writer
                .write_event(Event::End(BytesEnd::new("title")))
                .expect("writing start tag should succeed");
            for it in f.items.iter() {
                write_xbel_item(writer, it)
            }
            writer
                .write_event(Event::End(BytesEnd::new("folder")))
                .expect("writing start tag should succeed");
        }
        XbelItem::Bookmark(b) => {
            writer
                .write_event(Event::Start(
                    BytesStart::new("bookmark")
                        .with_attributes([("href", b.href.as_str()), ("id", b.id.as_str())]),
                ))
                .expect("writing start tag should succeed");
            writer
                .write_event(Event::Start(BytesStart::new("title")))
                .expect("writing start tag should succeed");
            writer
                .write_event(Event::Text(BytesText::new(b.title.text.as_str())))
                .expect("writing start tag should succeed");
            writer
                .write_event(Event::End(BytesEnd::new("title")))
                .expect("writing start tag should succeed");
            writer
                .write_event(Event::End(BytesEnd::new("bookmark")))
                .expect("writing start tag should succeed");
        }
    }
}

impl<'a> IntoIterator for &'a Xbel {
    type Item = &'a XbelItem;
    type IntoIter = XbelIterator<'a>;

    fn into_iter(self) -> Self::IntoIter {
        XbelIterator::new(self)
    }
}

/// A DFS Iterator for Xbel
pub struct XbelIterator<'s> {
    xbel: &'s Xbel,
    initial: bool,
    to_process: VecDeque<&'s XbelItem>,
}

impl<'s> XbelIterator<'s> {
    fn new(xbel: &'s Xbel) -> Self {
        Self {
            xbel,
            initial: true,
            to_process: Default::default(),
        }
    }
}

impl<'a> Iterator for XbelIterator<'a> {
    type Item = &'a XbelItem;

    fn next(&mut self) -> Option<Self::Item> {
        if self.initial {
            self.to_process.extend(self.xbel.items.iter());
            self.initial = false;
        }

        let xbel_item = self.to_process.pop_front()?;
        if let XbelItem::Folder(f) = xbel_item {
            for i in f.items.iter().rev() {
                self.to_process.push_front(i);
            }
        }

        Some(xbel_item)
    }
}

/// Either a `XbelItem` or the end of a Folder
#[derive(Debug)]
pub enum XbelItemOrEnd<'s> {
    Item(&'s XbelItem),
    #[allow(dead_code)]
    End(String), // id
}

/// A DFS Iterator for Xbel (but with nesting information)
pub struct XbelNestingIterator<'s> {
    xbel: &'s Xbel,
    initial: bool,
    to_process: VecDeque<XbelItemOrEnd<'s>>,
}

impl<'s> XbelNestingIterator<'s> {
    pub fn new(xbel: &'s Xbel) -> Self {
        Self {
            xbel,
            initial: true,
            to_process: Default::default(),
        }
    }
}

impl<'a> Iterator for XbelNestingIterator<'a> {
    type Item = XbelItemOrEnd<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.initial {
            self.to_process
                .extend(self.xbel.items.iter().map(XbelItemOrEnd::Item));
            self.initial = false;
        }

        let xbel_iter_item = self.to_process.pop_front()?;
        if let XbelItemOrEnd::Item(XbelItem::Folder(f)) = xbel_iter_item {
            self.to_process.push_front(XbelItemOrEnd::End(f.id.clone()));
            for i in f.items.iter().rev() {
                self.to_process.push_front(XbelItemOrEnd::Item(i));
            }
        }

        Some(xbel_iter_item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quick_xml::de::from_str;
    use tracing_test::traced_test;

    const XBEL_EMPTY: &str = r#"
            <?xml version="1.0" encoding="UTF-8"?>
            <!DOCTYPE xbel PUBLIC "+//IDN python.org//DTD XML Bookmark Exchange Language 1.0//EN//XML" "http://pyxml.sourceforge.net/topics/dtds/xbel.dtd">
            <xbel version="1.0">
            <!--- highestId :0: for Floccus bookmark sync browser extension -->
            </xbel>
        "#;
    const XBEL_BANK: &str = r#"
            <?xml version="1.0" encoding="UTF-8"?>
            <!DOCTYPE xbel PUBLIC "+//IDN python.org//DTD XML Bookmark Exchange Language 1.0//EN//XML" "http://pyxml.sourceforge.net/topics/dtds/xbel.dtd">
            <xbel version="1.0">
            <!--- highestId :5: for Floccus bookmark sync browser extension -->

            <folder id="1">
                <title>admin</title>
                <folder id="2">
                    <title>bank</title>
                    <bookmark href="https://www.bank1.com/" id="3">
                        <title>Bank 1 - Best bank in the world</title>
                    </bookmark>
                    <bookmark href="https://www.bank2.com" id="4">
                        <title>Bank 2 because 2 > 1 !#€</title>
                    </bookmark>
                </folder>
                <bookmark href="https://www.bank3.com" id="5">
                    <title>My current bank U+1F929 </title>
                </bookmark>
            </folder>
            </xbel>
        "#;

    // XXX:
    // Allow Clone for Folder/Bookmark (not only for tests)?
    impl Clone for Title {
        fn clone(&self) -> Self {
            Self {
                text: self.text.clone(),
            }
        }
    }

    impl Clone for Bookmark {
        fn clone(&self) -> Self {
            Self {
                href: self.href.clone(),
                id: self.id.clone(),
                title: self.title.clone(),
            }
        }
    }

    impl Clone for XbelItem {
        fn clone(&self) -> Self {
            match self {
                XbelItem::Folder(f) => XbelItem::Folder(f.clone()),
                XbelItem::Bookmark(b) => XbelItem::Bookmark(b.clone()),
            }
        }
    }

    impl Clone for Folder {
        fn clone(&self) -> Self {
            Self {
                id: self.id.clone(),
                title: self.title.clone(),
                items: self.items.clone(),
            }
        }
    }

    #[test]
    #[traced_test]
    fn read_xbel_empty() -> Result<(), quick_xml::errors::serialize::DeError> {
        // Try to read an empty xbel file
        let xbel: Xbel = from_str(XBEL_EMPTY)?;
        println!("xbel: {:?}", xbel);
        assert_eq!(xbel.items.len(), 0);
        Ok(())
    }

    #[test]
    #[traced_test]
    fn read_xbel_1() -> Result<(), quick_xml::errors::serialize::DeError> {
        // Try to read a valid xbel file
        let xbel: Xbel = from_str(XBEL_BANK)?;
        println!("xbel: {:?}", xbel);

        assert_eq!(xbel.items.len(), 1);
        assert!(matches!(xbel.items[0], XbelItem::Folder(..)));
        assert_eq!(xbel.items[0].get_title().text.as_str(), "admin");
        assert_eq!(xbel.items[0].get_id().as_str(), "1");

        // Check the first item is a Folder of id=2, followed by a Bookmark of id=5
        {
            if let XbelItem::Folder(f1) = &xbel.items[0] {
                if let XbelItem::Folder(f2) = &f1.items[0] {
                    assert_eq!(f2.id, "2");
                } else {
                    panic!("Expected a folder (with id 2)");
                }

                if let XbelItem::Bookmark(b5) = &f1.items[1] {
                    assert_eq!(b5.id, "5");
                } else {
                    panic!("Expected a bookmark (with id 4)")
                }
            } else {
                panic!("Expected a folder (with id 2)");
            }
        }

        Ok(())
    }

    #[test]
    #[traced_test]
    fn xbel_iter() -> Result<(), quick_xml::errors::serialize::DeError> {
        // Try to read a valid xbel file and to iterate over content
        let xbel: Xbel = from_str(XBEL_BANK)?;
        let mut xbel_it = XbelIterator::new(&xbel);

        let i = xbel_it.next().unwrap();
        assert_eq!(i.get_id(), "1");
        if let XbelItem::Folder(f1) = &i {
            assert_eq!(f1.title.text, "admin".to_string())
        } else {
            panic!("Expecting a folder");
        }
        let i = xbel_it.next().unwrap();
        assert_eq!(i.get_id(), "2");
        if let XbelItem::Folder(f2) = &i {
            assert_eq!(f2.title.text, "bank".to_string())
        } else {
            panic!("Expecting a folder");
        }
        let i = xbel_it.next().unwrap();
        assert_eq!(i.get_id(), "3");
        if let XbelItem::Bookmark(b3) = &i {
            assert_eq!(b3.href, "https://www.bank1.com/".to_string())
        } else {
            panic!("Expecting a folder");
        }
        let i = xbel_it.next().unwrap();
        assert_eq!(i.get_id(), "4");
        let i = xbel_it.next().unwrap();
        assert_eq!(i.get_id(), "5");
        assert_eq!(xbel_it.next().is_none(), true);

        let xbel_it2 = XbelIterator::new(&xbel);
        let bookmarks_only = xbel_it2
            .filter_map(|item| {
                if let XbelItem::Bookmark(b1) = &item {
                    Some(b1)
                } else {
                    None
                }
            })
            .collect::<Vec<&Bookmark>>();

        assert_eq!(bookmarks_only.len(), 3);

        Ok(())
    }

    #[test]
    #[traced_test]
    fn xbel_nesting_iter() -> Result<(), quick_xml::errors::serialize::DeError> {
        // Try to read a valid xbel file and to iterate over content
        let xbel: Xbel = from_str(XBEL_BANK)?;
        let mut xbel_it = XbelNestingIterator::new(&xbel);

        let i = xbel_it.next().unwrap();
        if let XbelItemOrEnd::Item(XbelItem::Folder(f1)) = &i {
            assert_eq!(f1.id, "1");
            assert_eq!(f1.title.text, "admin".to_string())
        } else {
            panic!("Expecting a folder");
        }
        let i = xbel_it.next().unwrap();
        if let XbelItemOrEnd::Item(XbelItem::Folder(f2)) = &i {
            assert_eq!(f2.id, "2");
            assert_eq!(f2.title.text, "bank".to_string())
        } else {
            panic!("Expecting a folder");
        }
        // bookmark id 3
        let _i = xbel_it.next().unwrap();
        // bookmark id 4
        let _i = xbel_it.next().unwrap();
        let i = xbel_it.next().unwrap();
        if let XbelItemOrEnd::End(folder_id) = &i {
            assert_eq!(folder_id, "2");
        } else {
            panic!("Expecting a end folder marker");
        }
        // bookmark id 5
        let _i = xbel_it.next().unwrap();
        let i = xbel_it.next().unwrap();
        if let XbelItemOrEnd::End(folder_id) = &i {
            assert_eq!(folder_id, "1");
        } else {
            panic!("Expecting a end folder marker");
        }

        Ok(())
    }

    #[test]
    #[traced_test]
    fn xbel_highest_id() -> Result<(), quick_xml::errors::serialize::DeError> {
        // Try to read a valid xbel file and to iterate over content
        let xbel: Xbel = from_str(XBEL_BANK)?;
        assert_eq!(xbel.get_highest_id(), 5);
        Ok(())
    }

    #[test]
    #[traced_test]
    fn add_xbel_empty() -> Result<(), quick_xml::errors::serialize::DeError> {
        // Add bookmark to empty Xbel
        let mut xbel: Xbel = from_str(XBEL_EMPTY)?;
        println!("xbel: {:?}", xbel);
        assert_eq!(xbel.get_highest_id(), 0);
        let bookmark_id = (xbel.get_highest_id() + 1).to_string();
        let items_0 = xbel.get_items_mut(&XbelPath::Id(1));
        assert!(items_0.is_none());
        let (item_index, items) = xbel.get_items_mut(&XbelPath::Root).unwrap();
        assert_eq!(item_index, 0);
        println!("items: {:?}", items);
        let bookmark = Bookmark::new(
            bookmark_id.as_str(),
            "https://www.example_bank.com",
            "Example bank",
        );
        items.push(XbelItem::Bookmark(bookmark));
        println!("xbel: {:?}", xbel);
        Ok(())
    }

    #[test]
    #[traced_test]
    fn add_xbel_1() -> Result<(), quick_xml::errors::serialize::DeError> {
        let mut xbel: Xbel = from_str(XBEL_BANK)?;
        println!("xbel: {:?}", xbel);
        let bookmark = xbel.new_bookmark("https://www.example_bank.com", "Example bank");
        let (item_index, items) = xbel.get_items_mut(&XbelPath::Id(4)).unwrap();
        println!("items: {:?}", items);
        assert_eq!(item_index, 1); // bookmark id == 4 has index == 1 in folder "bank"
        items.push(bookmark);
        println!("xbel: {:?}", xbel);
        Ok(())
    }

    #[test]
    #[traced_test]
    fn write_xbel() -> Result<(), quick_xml::errors::serialize::DeError> {

        let bookmark_b1 = XbelItem::Bookmark(Bookmark::new(
            "3",
            "https://www.bank1.com/",
            "Bank 1 - Best bank in the world",
        ));
        let bookmark_b2 = XbelItem::Bookmark(Bookmark::new(
            "4",
            "https://www.bank2.com/",
            "Bank 2 because 2 gt 1 !#€",
        ));
        let folder_i2 = XbelItem::Folder(Folder::new(
            "2",
            "bank",
            Some(vec![bookmark_b1, bookmark_b2]),
        ));
        let folder_i1 = XbelItem::Folder(Folder::new("1", "admin", Some(vec![folder_i2])));

        let xbel = Xbel::new(Some(vec![folder_i1]));

        let buffer = xbel.to_string();
        // println!("buffer:");
        // println!("{}", buffer);
        let bank_v1 = std::fs::read_to_string("../ressources/bookmarks_bank_v1.xbel").unwrap();
        assert_eq!(buffer, bank_v1);

        /*
        assert!(buffer.starts_with(xbel.xml_header()));
        assert!(buffer.find(url_e).is_some());
        assert!(buffer.find(url_g).is_some());
        assert!(buffer.find(title_g).is_some());
        assert!(buffer.find(url_b).is_some());
        */

        Ok(())
    }

    /*
    #[test]
    fn write_xbel_ser() -> Result<(), quick_xml::errors::serialize::DeError> {
        let url_e = "www.ecosia.org";
        let bookmark_e = XbelItem::Bookmark(Bookmark::new("1", url_e, "My main search engine"));
        let url_g = "www.google.com";
        let title_g = "A good search engine";
        let bookmark_g = XbelItem::Bookmark(Bookmark::new("4", url_g, title_g));
        let url_b = "www.bing.com";
        let bookmark_b =
            XbelItem::Bookmark(Bookmark::new("5", url_b, "Another good search engine"));

        let folder_a = XbelItem::Folder(Folder::new(
            "2",
            "Search engines",
            Some(vec![bookmark_g, bookmark_b]),
        ));

        let xbel = Xbel::new(Some(vec![bookmark_e, folder_a]));

        let mut buffer_ = String::new();
        let mut ser = quick_xml::se::Serializer::new(&mut buffer_);
        ser.indent(' ', 2);
        xbel.serialize(ser)?;
        // Add xml header + the xml highest id (as a xml comment)
        let buffer = xbel.add_header(&buffer_);

        println!("buffer:");
        println!("{}", buffer);

        assert!(buffer.starts_with(xbel.xml_header()));
        assert!(buffer.find(url_e).is_some());
        assert!(buffer.find(url_g).is_some());
        assert!(buffer.find(title_g).is_some());
        assert!(buffer.find(url_b).is_some());

        Ok(())
    }
    */
}
