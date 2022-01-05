// Copyright (C) 2019-2020 Alibaba Cloud, Red Hat, Inc and Amazon.com, Inc. or its affiliates.
// All Rights Reserved.

// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Traits and Structs to manage interrupt sources for devices.
//!
//! In system programming, an interrupt is a signal to the processor emitted by hardware or
//! software indicating an event that needs immediate attention. An interrupt alerts the processor
//! to a high-priority condition requiring the interruption of the current code the processor is
//! executing. The processor responds by suspending its current activities, saving its state, and
//! executing a function called an interrupt handler (or an interrupt service routine, ISR) to deal
//! with the event. This interruption is temporary, and, after the interrupt handler finishes,
//! unless handling the interrupt has emitted a fatal error, the processor resumes normal
//! activities.
//!
//! Hardware interrupts are used by devices to communicate that they require attention from the
//! operating system, or a bare-metal program running on the CPU if there are no OSes. The act of
//! initiating a hardware interrupt is referred to as an interrupt request (IRQ). Different devices
//! are usually associated with different interrupts using a unique value associated with each
//! interrupt. This makes it possible to know which hardware device caused which interrupts.
//! These interrupt values are often called IRQ lines, or just interrupt lines.
//!
//! Nowadays, IRQ lines is not the only mechanism to deliver device interrupts to processors.
//! MSI [(Message Signaled Interrupt)](https://en.wikipedia.org/wiki/Message_Signaled_Interrupts)
//! is another commonly used alternative in-band method of signaling an interrupt, using special
//! in-band messages to replace traditional out-of-band assertion of dedicated interrupt lines.
//! While more complex to implement in a device, message signaled interrupts have some significant
//! advantages over pin-based out-of-band interrupt signaling. Message signaled interrupts are
//! supported in PCI bus since its version 2.2, and in later available PCI Express bus. Some non-PCI
//! architectures also use message signaled interrupts.
//!
//! While IRQ is a term commonly used by Operating Systems when dealing with hardware
//! interrupts, the IRQ numbers managed by OSes are independent of the ones managed by VMM.
//! For simplicity sake, the term `Interrupt Source` is used instead of IRQ to represent both pin-based
//! interrupts and MSI interrupts.
//!
//! A device may support multiple types of interrupts, and each type of interrupt may support one
//! or multiple interrupt sources. For example, a PCI device may support:
//! * Legacy Irq: exactly one interrupt source.
//! * PCI MSI Irq: 1,2,4,8,16,32 interrupt sources.
//! * PCI MSIx Irq: 2^n(n=0-11) interrupt sources.

pub mod legacy;
pub mod msi;

use std::fmt::{self, Display};
use std::ops::Deref;

/// Errors associated with handling interrupts
#[derive(Debug)]
pub enum Error {
    /// Operation not supported for this interrupt.
    OperationNotSupported,

    /// The specified configuration is not valid.
    InvalidConfiguration,

    /// The interrupt was not enabled.
    InterruptNotChanged,

    /// The interrupt could not be triggered.
    InterruptNotTriggered,
}

impl std::error::Error for Error {}

/// Reuse std::io::Result to simplify interoperability among crates.
pub type Result<T> = std::result::Result<T, Error>;

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Interrupt error: ")?;
        match self {
            Error::OperationNotSupported => write!(f, "operation not supported"),
            Error::InvalidConfiguration => write!(f, "invalid configuration"),
            Error::InterruptNotChanged => write!(f, "the interrupt state could not be changed"),
            Error::InterruptNotTriggered => write!(f, "the interrupt could not be triggered"),
        }
    }
}

/// Trait used by interrupt producers to signal interrupts.
///
/// An object having the Interrupt Trait is shared between the VMM and the device
/// that is using the interrupt.
/// The `Interrupt` performs two main goals:
///     * offers a control interface for the interrupt through the `enable()`
///     and `disable()` methods;
///     * offers a channel through which notifications can be passed between
///     the VMM and the device;
///
/// When an `Interrupt` is triggered, a notification mechanism that is known by the
/// Hypervisor will be used to signal the guest. The type of the notification mechanism
/// is defined by the `NotifierType` associated type.
/// `Interrupt` allows access to the undelying mechanism used by the Hypervisor through
/// the `notifier()`. This enables some use cases the device may want to bypass the VMM
/// completely or when the device crate acts only as a control plane and the actual
/// emulation is implemented in some other component that understands the underlying
/// mechanism.
/// A notable example is VFIO that allows a device to register the irqfd so that
/// interrupts follow a fast path that doesn't require going through the VMM.
///
/// Objects implementing this trait are required to have internal mutability.
pub trait Interrupt {
    /// The type of the underlying mechanism used for notifications by this interrupt.
    type NotifierType;

    /// Inject an interrupt from this interrupt source into the guest.
    fn trigger(&self) -> Result<()>;

    /// Returns an interrupt notifier from this interrupt.
    ///
    /// An interrupt notifier allows for external components and processes
    /// to inject interrupts into a guest through a different interface other
    /// than `trigger`.
    fn notifier(&self) -> Option<Self::NotifierType> {
        None
    }

    /// Called back when the CPU acknowledges the interrupt.
    fn acknowledge(&self) -> Result<()> {
        Err(Error::OperationNotSupported)
    }

    /// Returns an end-of-interrupt notifier from this interrupt.
    ///
    /// An end-of-interrupt notifier allows for external components and processes
    /// to be notified when a guest acknowledges an interrupt. This can be used
    /// to resample and inject a level-triggered interrupt, or to mitigate the
    /// effect of lost timer interrupts.
    fn ack_notifier(&self) -> Option<Self::NotifierType> {
        None
    }

    /// Enable generation of interrupts from this interrupt source.
    fn enable(&self) -> Result<()> {
        Err(Error::OperationNotSupported)
    }

    /// Disable generation of interrupts from this interrupt source.
    fn disable(&self) -> Result<()> {
        Err(Error::OperationNotSupported)
    }
}

/// Trait for interrupts that allow users to configure interrupt parameters.
///
/// This enhances the control plane interface of the `Interrupt` by allowing
/// a device to configure the behavior of the interrupt.
pub trait ConfigurableInterrupt: Interrupt {
    /// Type describing the configuration spec of the interrupt.
    type Cfg;

    /// Update configuration of the interrupt.
    fn update(&self, config: &Self::Cfg) -> Result<()>;

    /// Returns the current configuration of the interrupt.
    fn get_config(&self) -> Result<Self::Cfg>;
}

/// Trait for interrupts that can be masked or unmasked.
pub trait MaskableInterrupt: Interrupt {
    /// Mask the interrupt.  Masked interrupts are remembered but
    /// not delivered.
    fn mask(&self) -> Result<()>;

    /// Unmask the interrupt, delivering it if it was pending.
    fn unmask(&self) -> Result<()>;
}

/// Trait to manage a group of interrupt sources for a device.
///
/// A device may use an InterruptSourceGroup to manage multiple interrupts of the same type.
/// The group allows a device to request and release interrupts and perform actions on the
/// whole collection of interrupts like enable and disable for cases where enabling or disabling
/// a single interrupt in the group does not make sense. For example, PCI MSI interrupts must be
/// enabled as a group.
pub trait InterruptSourceGroup: Send {
    /// Type of the interrupts contained in this group.
    type InterruptType: Interrupt;

    /// Interrupt Type returned by get
    type InterruptWrapper: Deref<Target = Self::InterruptType>;

    /// Return whether the group manages no interrupts.
    fn is_empty(&self) -> bool;

    /// Get number of interrupt sources managed by the group.
    fn len(&self) -> usize;

    /// Enable the interrupt sources in the group to generate interrupts.
    fn enable(&self) -> Result<()>;

    /// Disable the interrupt sources in the group to generate interrupts.
    fn disable(&self) -> Result<()>;

    /// Return the index-th interrupt in the group, or `None` if the index is out
    /// of bounds.
    fn get(&self, index: usize) -> Option<Self::InterruptWrapper>;

    /// Request new interrupts within this group.
    fn allocate_interrupts(&mut self, size: usize) -> Result<()>;

    /// Release all interrupts within this group.
    fn free_interrupts(&mut self) -> Result<()>;
}
