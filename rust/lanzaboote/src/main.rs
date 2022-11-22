#![no_main]
#![no_std]
#![feature(abi_efiapi)]

extern crate alloc;

mod pe_section;
mod uefi_helpers;

use log::{debug, info};
use uefi::{
    prelude::*,
    proto::{
        console::text::Output,
        device_path::text::{AllowShortcuts, DevicePathToText, DisplayOnly},
        loaded_image::LoadedImage,
        media::file::{File, FileAttribute, FileMode, RegularFile},
    },
    table::boot::{OpenProtocolAttributes, OpenProtocolParams},
    Result,
};

use crate::{pe_section::pe_section, uefi_helpers::read_all};

fn print_logo(output: &mut Output) {
    output.clear().unwrap();

    output
        .output_string(cstr16!(
            "
  _                      _                 _   \r
 | |                    | |               | |  \r
 | | __ _ _ __  ______ _| |__   ___   ___ | |_ \r
 | |/ _` | '_ \\|_  / _` | '_ \\ / _ \\ / _ \\| __|\r
 | | (_| | | | |/ / (_| | |_) | (_) | (_) | |_ \r
 |_|\\__,_|_| |_/___\\__,_|_.__/ \\___/ \\___/ \\__|\r
\r
"
        ))
        .unwrap();
}

fn image_file(boot_services: &BootServices, image: Handle) -> Result<RegularFile> {
    let mut file_system = boot_services.get_image_file_system(image)?;
    let mut root = file_system.open_volume()?;

    let loaded_image = unsafe {
        // XXX This gives ACCESS_DENIED if we use open_protocol_exclusive?
        boot_services.open_protocol::<LoadedImage>(
            OpenProtocolParams {
                handle: image,
                agent: image,
                controller: None,
            },
            OpenProtocolAttributes::Exclusive,
        )
    }?;

    let file_path = loaded_image.file_path().ok_or(Status::NOT_FOUND)?;

    let to_text = boot_services.open_protocol_exclusive::<DevicePathToText>(
        boot_services.get_handle_for_protocol::<DevicePathToText>()?,
    )?;

    let file_path = to_text.convert_device_path_to_text(
        boot_services,
        file_path,
        DisplayOnly(false),
        AllowShortcuts(false),
    )?;

    let our_image = root.open(&file_path, FileMode::Read, FileAttribute::empty())?;

    Ok(our_image
        .into_regular_file()
        .ok_or(Status::INVALID_PARAMETER)?)
}

#[entry]
fn main(handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    uefi_services::init(&mut system_table).unwrap();

    print_logo(system_table.stdout());

    let boot_services = system_table.boot_services();

    {
        let image_data = read_all(&mut image_file(boot_services, handle).unwrap()).unwrap();

        if let Some(data) = pe_section(&image_data, ".osrel") {
            info!("osrel = {}", core::str::from_utf8(data).unwrap_or("???"))
        }
    }

    let mut file_system = boot_services.get_image_file_system(handle).unwrap();
    let mut root = file_system.open_volume().unwrap();

    let mut file = root
        .open(cstr16!("linux.efi"), FileMode::Read, FileAttribute::empty())
        .unwrap()
        .into_regular_file()
        .unwrap();

    debug!("Opened file");

    let kernel = read_all(&mut file).unwrap();

    let kernel_image = boot_services
        .load_image(
            handle,
            uefi::table::boot::LoadImageSource::FromBuffer {
                buffer: &kernel,
                file_path: None,
            },
        )
        .unwrap();

    boot_services.start_image(kernel_image).unwrap();

    Status::SUCCESS
}