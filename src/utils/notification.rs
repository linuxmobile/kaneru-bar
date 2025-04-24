use zbus::zvariant::{OwnedValue, Type};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Type)]
#[zvariant(signature = "y")]
pub enum Urgency {
    Low = 0,
    Normal = 1,
    Critical = 2,
}

impl TryFrom<OwnedValue> for Urgency {
    type Error = zbus::zvariant::Error;

    fn try_from(value: OwnedValue) -> Result<Self, Self::Error> {
        let byte_value = u8::try_from(value)?;
        match byte_value {
            0 => Ok(Urgency::Low),
            1 => Ok(Urgency::Normal),
            2 => Ok(Urgency::Critical),
            _ => Err(zbus::zvariant::Error::IncorrectType),
        }
    }
}

impl<'a> TryFrom<&zbus::zvariant::Value<'a>> for Urgency {
    type Error = zbus::zvariant::Error;

    fn try_from(value: &zbus::zvariant::Value<'a>) -> Result<Self, Self::Error> {
        let byte_value = value.try_into()?;
        match byte_value {
            0u8 => Ok(Urgency::Low),
            1u8 => Ok(Urgency::Normal),
            2u8 => Ok(Urgency::Critical),
            _ => Err(zbus::zvariant::Error::IncorrectType),
        }
    }
}

#[derive(Debug, Clone, Type)]
#[zvariant(signature = "a{sv}")]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub replaces_id: u32,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<String>,
    pub expire_timeout: i32,
    pub urgency: Urgency,
    pub image_path: Option<String>,
    pub resident: bool,
}

impl Notification {
    pub fn new(
        id: u32,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        expire_timeout: i32,
        urgency: Urgency,
        image_path: Option<String>,
        resident: bool,
    ) -> Self {
        Self {
            id,
            app_name,
            replaces_id,
            app_icon,
            summary,
            body,
            actions,
            expire_timeout,
            urgency,
            image_path,
            resident,
        }
    }
}
