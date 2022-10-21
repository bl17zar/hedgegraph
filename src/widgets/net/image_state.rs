use egui::ColorImage;

#[derive(Default, Clone)]
pub struct ImageState {
    changed: bool,
    image: ColorImage,
}

impl ImageState {
    pub fn update(&mut self, new_image: ColorImage) {
        self.image = new_image;
        self.changed = true;
    }

    pub fn changed(&self) -> bool {
        self.changed
    }

    pub fn image(&mut self) -> ColorImage {
        self.image.clone()
    }

    pub fn set_changed(&mut self, changed: bool) {
        self.changed = changed
    }
}
